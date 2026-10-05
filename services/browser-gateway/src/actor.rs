use crate::{
    eod::EodGate,
    native,
    server::{Session, Shared, capabilities},
};
use axum::extract::ws::{Message, WebSocket};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{net::tcp::OwnedWriteHalf, sync::mpsc, task::JoinHandle};
use wonderland_game_services::{protocol::*, *};

struct Peer {
    writer: OwnedWriteHalf,
    reader: JoinHandle<()>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
struct NativeEvent {
    generation: u64,
    lot: bool,
    packet: ServiceResult<Packet>,
}
struct Actor {
    socket: WebSocket,
    state: Arc<Shared>,
    session: Arc<Session>,
    projection: SessionProjection,
    ledger: OperationLedger,
    pending: HashMap<String, (GatewayOperation, tokio::time::Instant)>,
    city: Option<Peer>,
    lot: Option<Peer>,
    generation: u64,
    lot_generation: u64,
    tx: mpsc::Sender<NativeEvent>,
    rx: mpsc::Receiver<NativeEvent>,
    timer: tokio::time::Interval,
    eod: EodGate,
    last_snapshot_request: Option<tokio::time::Instant>,
    neighborhood_failed: bool,
    cancelled_find_lot: bool,
}

pub(crate) async fn run(socket: WebSocket, state: Arc<Shared>, session: Arc<Session>) {
    let projection = session.projection.lock().await.clone();
    let (tx, rx) = mpsc::channel(32);
    let mut ended = session.ended.subscribe();
    let mut actor = Actor {
        ledger: OperationLedger::new(projection.epoch),
        socket,
        state,
        session: session.clone(),
        projection,
        pending: HashMap::new(),
        city: None,
        lot: None,
        generation: 0,
        lot_generation: 0,
        tx,
        rx,
        timer: tokio::time::interval(Duration::from_secs(10)),
        eod: EodGate::new(0),
        last_snapshot_request: None,
        neighborhood_failed: false,
        cancelled_find_lot: false,
    };
    if actor
        .send(
            None,
            GatewayEvent::Session {
                session: actor.projection.clone(),
            },
        )
        .await
        .is_ok()
    {
        loop {
            if *ended.borrow() {
                break;
            }
            tokio::select! {
                _=ended.changed()=>break,
                _=tokio::time::sleep_until(session.expires)=>break,
                result=actor.step()=>if result.is_err(){break;},
            }
        }
    }
    actor.city = None;
    actor.lot = None;
    if actor.cancelled_find_lot {
        actor.complete("find_lot");
    }
    for id in actor.ledger.reset(actor.projection.epoch) {
        let _ = actor
            .send(
                Some(id),
                GatewayEvent::Outcome {
                    status: OutcomeStatus::Unknown,
                    source_code: None,
                    data: serde_json::Value::Null,
                    error: Some(native::disconnected()),
                },
            )
            .await;
    }
    actor.projection.state = SessionState::Disconnected;
    actor.projection.avatar_id = None;
    actor.projection.shard_name = None;
    actor.projection.lot_location = None;
    actor.projection.lot_incarnation = None;
    actor.projection.capabilities = capabilities(&actor.projection.state, None, None, true);
    *session.projection.lock().await = actor.projection.clone();
    let _ = actor
        .send(
            None,
            GatewayEvent::Session {
                session: actor.projection.clone(),
            },
        )
        .await;
    let _ = actor.socket.send(Message::Close(None)).await;
}

impl Actor {
    async fn send(&mut self, id: Option<String>, event: GatewayEvent) -> ServiceResult<()> {
        let text = serde_json::to_string(&GatewayEnvelope {
            epoch: self.projection.epoch,
            operation_id: id,
            event,
        })
        .map_err(|_| {
            ServiceError::new(
                ErrorCode::InvalidResponse,
                "Gateway could not encode its response",
            )
        })?;
        tokio::time::timeout(
            Duration::from_secs(5),
            self.socket.send(Message::Text(text.into())),
        )
        .await
        .map_err(|_| native::disconnected())?
        .map_err(|_| native::disconnected())
    }
    async fn update_session(&mut self) -> ServiceResult<()> {
        self.projection.capabilities = capabilities(
            &self.projection.state,
            self.projection.avatar_id,
            self.eod.active_plugin(),
            true,
        );
        if self.cancelled_find_lot
            && let Some(capability) = self
                .projection
                .capabilities
                .iter_mut()
                .find(|cap| cap.capability == "lot")
        {
            capability.available = false;
            capability.reason =
                Some("Waiting for the canceled original lot admission response".into());
        }
        *self.session.projection.lock().await = self.projection.clone();
        self.send(
            None,
            GatewayEvent::Session {
                session: self.projection.clone(),
            },
        )
        .await
    }
    async fn outcome(
        &mut self,
        id: String,
        status: OutcomeStatus,
        code: Option<u16>,
        data: serde_json::Value,
        error: Option<ServiceError>,
    ) -> ServiceResult<()> {
        self.send(
            Some(id),
            GatewayEvent::Outcome {
                status,
                source_code: code,
                data,
                error,
            },
        )
        .await
    }
    async fn step(&mut self) -> ServiceResult<()> {
        tokio::select! {
            message=self.socket.recv()=>match message {
                Some(Ok(Message::Text(text)))=>match serde_json::from_str::<BrowserMessage>(&text) {
                    Ok(BrowserMessage::Request{epoch,operation_id,operation})=>self.request(epoch,operation_id,operation).await,
                    _=>{self.send(None,GatewayEvent::Error{error:ServiceError::new(ErrorCode::InvalidRequest,"Malformed gateway operation")}).await?;Err(native::disconnected())}
                },
                Some(Ok(Message::Ping(data)))=>self.socket.send(Message::Pong(data)).await.map_err(|_|native::disconnected()),
                Some(Ok(Message::Pong(_)))=>Ok(()),
                _=>Err(native::disconnected()),
            },
            Some(event)=self.rx.recv()=>{
                if (event.lot && event.generation!=self.lot_generation) || (!event.lot && event.generation!=self.generation){return Ok(());}
                self.received(event.lot,event.packet?).await
            },
            _=self.timer.tick()=>{
                if self.pending.values().any(|(_,time)|time.elapsed()>Duration::from_secs(30)){return Err(ServiceError::new(ErrorCode::Timeout,"Original operation outcome is unknown after timeout"));}
                if let Some(city)=self.city.as_mut(){native::write_packet(&mut city.writer,&encode_packet(1000,13,&[])?).await?;}
                if let Some(lot)=self.lot.as_mut(){native::write_packet(&mut lot.writer,&encode_packet(1000,13,&[])?).await?;}
                Ok(())
            }
        }
    }
    async fn request(
        &mut self,
        epoch: u64,
        id: String,
        operation: GatewayOperation,
    ) -> ServiceResult<()> {
        if epoch != self.projection.epoch {
            return self
                .outcome(
                    id,
                    OutcomeStatus::Rejected,
                    None,
                    serde_json::Value::Null,
                    Some(ServiceError::new(
                        ErrorCode::StaleEpoch,
                        "The operation belongs to an expired session",
                    )),
                )
                .await;
        }
        let result = self.execute(&id, &operation).await;
        if let Err(error) = result {
            if matches!(error.code, ErrorCode::Disconnected | ErrorCode::Timeout) {
                return Err(error);
            }
            self.outcome(
                id,
                OutcomeStatus::Rejected,
                None,
                serde_json::Value::Null,
                Some(error),
            )
            .await?;
        }
        Ok(())
    }
    fn begin(&mut self, id: &str, family: &str, operation: &GatewayOperation) -> ServiceResult<()> {
        self.ledger.begin(self.projection.epoch, id, family)?;
        self.pending.insert(
            family.into(),
            (operation.clone(), tokio::time::Instant::now()),
        );
        Ok(())
    }
    fn complete(&mut self, family: &str) -> Option<String> {
        self.pending.remove(family);
        self.ledger.complete(self.projection.epoch, family)
    }
    async fn execute(&mut self, id: &str, operation: &GatewayOperation) -> ServiceResult<()> {
        match operation {
            GatewayOperation::RefreshRoster => {
                self.begin(id, "roster", operation)?;
                self.send(
                    Some(id.into()),
                    GatewayEvent::Pending {
                        family: "roster".into(),
                    },
                )
                .await?;
                let result = self
                    .state
                    .upstream
                    .as_ref()
                    .unwrap()
                    .roster(&self.session.grant.token)
                    .await;
                self.complete("roster");
                let roster = result?;
                *self.session.roster.write().await = roster.clone();
                self.send(Some(id.into()), GatewayEvent::Roster { roster })
                    .await?;
                self.outcome(
                    id.into(),
                    OutcomeStatus::Accepted,
                    None,
                    serde_json::Value::Null,
                    None,
                )
                .await
            }
            GatewayOperation::ConnectCity {
                shard_name,
                avatar_id,
            } => {
                if self.city.is_some() {
                    return Err(ServiceError::new(
                        ErrorCode::SessionConflict,
                        "Leave the current city before selecting another avatar",
                    ));
                }
                if *avatar_id != 0
                    && !self
                        .session
                        .roster
                        .read()
                        .await
                        .iter()
                        .any(|a| a.avatar_id == *avatar_id && a.shard_name == *shard_name)
                {
                    return Err(ServiceError::new(
                        ErrorCode::Unauthorized,
                        "This avatar is not present in the authenticated account roster",
                    ));
                }
                self.begin(id, "city_admission", operation)?;
                self.projection.state = SessionState::CityConnecting;
                self.projection.avatar_id = Some(*avatar_id);
                self.projection.shard_name = Some(shard_name.clone());
                self.update_session().await?;
                self.send(
                    Some(id.into()),
                    GatewayEvent::Pending {
                        family: "city_admission".into(),
                    },
                )
                .await?;
                let selected = self
                    .state
                    .upstream
                    .as_ref()
                    .unwrap()
                    .select_city(&self.session.grant.token, shard_name, *avatar_id)
                    .await;
                let connected = match selected {
                    Ok(selected) if selected.avatar_id == *avatar_id => {
                        native::connect(
                            &self.state.config.destinations,
                            &selected.address,
                            &selected.avatar_id.to_string(),
                            &selected.ticket,
                            true,
                        )
                        .await
                    }
                    Ok(_) => Err(ServiceError::new(
                        ErrorCode::InvalidResponse,
                        "City selection returned a different avatar",
                    )),
                    Err(error) => Err(error),
                };
                self.complete("city_admission");
                match connected {
                    Ok(connection) => {
                        self.generation += 1;
                        self.city = Some(spawn_peer(
                            connection,
                            self.generation,
                            false,
                            self.tx.clone(),
                        ));
                        self.projection.state = SessionState::CityReady;
                        self.update_session().await?;
                        self.outcome(
                            id.into(),
                            OutcomeStatus::Accepted,
                            None,
                            serde_json::json!({"admission":"city_protocol"}),
                            None,
                        )
                        .await
                    }
                    Err(error) => {
                        self.projection.state = SessionState::Authenticated;
                        self.projection.avatar_id = None;
                        self.projection.shard_name = None;
                        self.update_session().await?;
                        self.outcome(
                            id.into(),
                            OutcomeStatus::Rejected,
                            None,
                            serde_json::Value::Null,
                            Some(error),
                        )
                        .await
                    }
                }
            }
            GatewayOperation::DisconnectCity => {
                self.begin(id, "disconnect_city", operation)?;
                self.close_lot();
                self.city = None;
                self.generation += 1;
                self.complete("disconnect_city");
                self.abandon_pending().await?;
                self.projection.state = SessionState::Authenticated;
                self.projection.avatar_id = None;
                self.projection.shard_name = None;
                self.update_session().await?;
                self.outcome(
                    id.into(),
                    OutcomeStatus::Accepted,
                    None,
                    serde_json::json!({"disconnected":true}),
                    None,
                )
                .await
            }
            GatewayOperation::LeaveLot => {
                self.begin(id, "leave_lot", operation)?;
                let awaiting_city_reply =
                    self.lot.is_none() && self.pending.contains_key("find_lot");
                self.close_lot();
                self.complete("leave_lot");
                for family in ["find_lot", "world_snapshot"] {
                    let operation_id = if family == "find_lot" && awaiting_city_reply {
                        if self.cancelled_find_lot {
                            None
                        } else {
                            self.cancelled_find_lot = true;
                            self.ledger
                                .operation_id(self.projection.epoch, family)
                                .map(str::to_owned)
                        }
                    } else {
                        self.complete(family)
                    };
                    if let Some(id) = operation_id {
                        self.outcome(
                            id,
                            OutcomeStatus::Unknown,
                            None,
                            serde_json::Value::Null,
                            Some(native::disconnected()),
                        )
                        .await?;
                    }
                }
                self.projection.state = if self.city.is_some() {
                    SessionState::CityReady
                } else {
                    SessionState::Authenticated
                };
                self.update_session().await?;
                self.outcome(
                    id.into(),
                    OutcomeStatus::Accepted,
                    None,
                    serde_json::json!({"left_lot":true}),
                    None,
                )
                .await
            }
            _ => {
                if self.city.is_none() {
                    return Err(ServiceError::new(
                        ErrorCode::Unauthorized,
                        "An authenticated city connection is required",
                    ));
                }
                let actor = self.projection.avatar_id.unwrap_or(0);
                if matches!(operation, GatewayOperation::CreateAvatar { .. }) {
                    if actor != 0 {
                        return Err(ServiceError::new(
                            ErrorCode::Unauthorized,
                            "Character creation requires the original anonymous create-avatar city session",
                        ));
                    }
                } else if actor == 0 {
                    return Err(ServiceError::new(
                        ErrorCode::Unauthorized,
                        "A selected account avatar is required",
                    ));
                }
                if let GatewayOperation::JoinLot { .. } = operation
                    && self.lot.is_some()
                {
                    return Err(ServiceError::new(
                        ErrorCode::SessionConflict,
                        "Leave the current lot before joining another",
                    ));
                }
                let actor_name = self
                    .session
                    .roster
                    .read()
                    .await
                    .iter()
                    .find(|a| a.avatar_id == actor)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                let encoded = match operation {
                    GatewayOperation::CancelInteraction {
                        lot_incarnation, ..
                    }
                    | GatewayOperation::WalkTo {
                        lot_incarnation, ..
                    } => {
                        self.require_lot(*lot_incarnation)?;
                        encode_operation(operation, actor, &actor_name, id)?
                    }
                    GatewayOperation::LotCommand {
                        lot_incarnation,
                        data,
                    } => {
                        self.require_lot(*lot_incarnation)?;
                        wonderland_player_authoring::validate_client_command(data,actor).map_err(|_|ServiceError::new(ErrorCode::InvalidRequest,"The supplied original command is malformed, claims another actor, or contains server-only fields"))?;
                        EncodedOperation {
                            bytes: wrap_vm_command(data)?,
                            family: "lot_command".into(),
                            has_response: false,
                            lot: true,
                        }
                    }
                    GatewayOperation::Eod {
                        incarnation,
                        plugin_id,
                        ..
                    } => {
                        self.eod.authorize(*incarnation, *plugin_id)?;
                        encode_operation(operation, actor, &actor_name, id)?
                    }
                    GatewayOperation::RequestWorldSnapshot { lot_incarnation } => {
                        self.require_lot(*lot_incarnation)?;
                        if self
                            .last_snapshot_request
                            .is_some_and(|time| time.elapsed() < Duration::from_secs(5))
                        {
                            return Err(ServiceError::new(
                                ErrorCode::ResourceBusy,
                                "Please wait before requesting another world snapshot",
                            ));
                        }
                        EncodedOperation {
                            bytes: request_world_snapshot(self.eod.last_tick().unwrap_or(0))?,
                            family: "world_snapshot".into(),
                            has_response: true,
                            lot: true,
                        }
                    }
                    _ => encode_operation(operation, actor, &actor_name, id)?,
                };
                if encoded.lot && self.projection.state != SessionState::LotReady {
                    return Err(ServiceError::new(
                        ErrorCode::Unauthorized,
                        "An admitted lot connection is required",
                    ));
                }
                self.begin(id, &encoded.family, operation)?;
                self.send(
                    Some(id.into()),
                    GatewayEvent::Pending {
                        family: encoded.family.clone(),
                    },
                )
                .await?;
                let peer = if encoded.lot {
                    self.lot.as_mut()
                } else {
                    self.city.as_mut()
                }
                .ok_or_else(native::disconnected)?;
                native::write_packet(&mut peer.writer, &encoded.bytes).await?;
                if matches!(operation, GatewayOperation::RequestWorldSnapshot { .. }) {
                    self.last_snapshot_request = Some(tokio::time::Instant::now());
                }
                if matches!(operation, GatewayOperation::JoinLot { .. }) {
                    self.projection.state = SessionState::LotConnecting;
                    self.update_session().await?;
                }
                if !encoded.has_response {
                    self.complete(&encoded.family);
                    self.outcome(id.into(),OutcomeStatus::Unknown,None,serde_json::json!({"submitted":true}),Some(ServiceError::new(ErrorCode::OperationPending,"The original protocol has no correlated acceptance receipt for this operation"))).await?;
                }
                Ok(())
            }
        }
    }
    fn require_lot(&self, incarnation: u64) -> ServiceResult<()> {
        if self.projection.state != SessionState::LotReady
            || self.projection.lot_incarnation != Some(incarnation)
        {
            return Err(ServiceError::new(
                ErrorCode::StaleEpoch,
                "The operation belongs to a different lot admission",
            ));
        }
        Ok(())
    }
    fn close_lot(&mut self) {
        self.lot = None;
        self.lot_generation += 1;
        self.projection.lot_location = None;
        self.projection.lot_incarnation = None;
        self.eod.reset(0);
        self.last_snapshot_request = None;
    }
    async fn abandon_pending(&mut self) -> ServiceResult<()> {
        if self.cancelled_find_lot {
            self.complete("find_lot");
            self.cancelled_find_lot = false;
        }
        self.pending.clear();
        self.neighborhood_failed = false;
        for id in self.ledger.reset(self.projection.epoch) {
            self.outcome(
                id,
                OutcomeStatus::Unknown,
                None,
                serde_json::Value::Null,
                Some(native::disconnected()),
            )
            .await?;
        }
        Ok(())
    }
    async fn received(&mut self, lot: bool, packet: Packet) -> ServiceResult<()> {
        match parse_packet(&packet)? {
            SourcePacket::ServerBye => Err(native::disconnected()),
            SourcePacket::FindLot {
                status,
                lot_location,
                ticket,
                address,
                user,
            } if !lot => {
                let matches=self.pending.get("find_lot").is_some_and(|(op,_)|matches!(op,GatewayOperation::JoinLot{lot_location:expected,..} if *expected==lot_location || (0x200..0x300).contains(expected)));
                if !matches {
                    return Ok(());
                }
                if self.cancelled_find_lot {
                    self.complete("find_lot");
                    self.cancelled_find_lot = false;
                    return self.update_session().await;
                }
                if status != 0 {
                    let id = self.complete("find_lot").unwrap();
                    self.projection.state = SessionState::CityReady;
                    self.update_session().await?;
                    return self
                        .outcome(
                            id,
                            OutcomeStatus::Rejected,
                            Some(status),
                            serde_json::json!({"lot_location":lot_location}),
                            Some(ServiceError::new(
                                ErrorCode::Rejected,
                                "The original city refused lot admission",
                            )),
                        )
                        .await;
                }
                if user.parse::<u32>().ok() != self.projection.avatar_id {
                    return Err(ServiceError::new(
                        ErrorCode::InvalidResponse,
                        "The original lot ticket belongs to a different avatar",
                    ));
                }
                let result = native::connect(
                    &self.state.config.destinations,
                    &address,
                    &user,
                    &ticket,
                    false,
                )
                .await;
                match result {
                    Ok(connection) => {
                        self.lot_generation += 1;
                        self.lot = Some(spawn_peer(
                            connection,
                            self.lot_generation,
                            true,
                            self.tx.clone(),
                        ));
                        self.projection.lot_location = Some(lot_location);
                        self.projection.lot_incarnation = Some(self.lot_generation);
                        self.eod.reset(self.projection.avatar_id.unwrap_or(0));
                        Ok(())
                    }
                    Err(error) => {
                        let id = self.complete("find_lot").unwrap();
                        self.projection.state = SessionState::CityReady;
                        self.update_session().await?;
                        self.outcome(
                            id,
                            OutcomeStatus::Rejected,
                            None,
                            serde_json::Value::Null,
                            Some(error),
                        )
                        .await
                    }
                }
            }
            SourcePacket::Response {
                family,
                source_code,
                accepted,
                data,
            } if !lot => {
                if family.starts_with("instant_message:")
                    && let Some((
                        GatewayOperation::PrivateMessage {
                            target_avatar_id, ..
                        },
                        _,
                    )) = self.pending.get(&family)
                    && data.get("from").and_then(serde_json::Value::as_u64)
                        != Some(u64::from(*target_avatar_id))
                {
                    return Ok(());
                }
                if family == "neighborhood" && self.neighborhood_failed {
                    if accepted {
                        self.complete(&family);
                        self.neighborhood_failed = false;
                    }
                    return Ok(());
                }
                if family == "neighborhood"
                    && matches!(source_code, 5 | 6)
                    && self.pending.get(&family).is_some_and(|(op, _)| {
                        matches!(
                            op,
                            GatewayOperation::Neighborhood {
                                action: NeighborhoodAction::CanVote
                                    | NeighborhoodAction::CanNominate,
                                ..
                            }
                        )
                    })
                {
                    self.neighborhood_failed = true;
                    if let Some(id) = self
                        .ledger
                        .operation_id(self.projection.epoch, &family)
                        .map(str::to_owned)
                    {
                        self.outcome(
                            id,
                            OutcomeStatus::Rejected,
                            Some(source_code),
                            data,
                            Some(ServiceError::new(
                                ErrorCode::Rejected,
                                "The original service rejected the operation",
                            )),
                        )
                        .await?;
                    }
                    return Ok(());
                }
                if let Some(id) = self.complete(&family) {
                    self.outcome(
                        id,
                        if accepted {
                            OutcomeStatus::Accepted
                        } else {
                            OutcomeStatus::Rejected
                        },
                        Some(source_code),
                        data,
                        if accepted {
                            None
                        } else {
                            Some(ServiceError::new(
                                ErrorCode::Rejected,
                                "The original service rejected the operation",
                            ))
                        },
                    )
                    .await
                } else {
                    self.send(
                        None,
                        GatewayEvent::SourceEvent {
                            family,
                            source_code: Some(source_code),
                            data,
                        },
                    )
                    .await
                }
            }
            SourcePacket::Event {
                family,
                source_code,
                mut data,
            } => {
                if family == "neighborhood_candidates" {
                    let context = self
                        .pending
                        .get("neighborhood")
                        .and_then(|(op, _)| match op {
                            GatewayOperation::Neighborhood {
                                action,
                                neighborhood_id,
                                ..
                            } if matches!(
                                action,
                                NeighborhoodAction::CanVote
                                    | NeighborhoodAction::CanNominate
                                    | NeighborhoodAction::CanFreeVote
                            ) =>
                            {
                                Some((*action, *neighborhood_id))
                            }
                            _ => None,
                        });
                    if let Some((action, neighborhood)) = context {
                        data["requested_neighborhood_id"] = serde_json::json!(neighborhood);
                        data["requested_action"] = serde_json::json!(action);
                        data["permission_rejected"] = serde_json::json!(self.neighborhood_failed);
                        let id = self
                            .ledger
                            .operation_id(self.projection.epoch, "neighborhood")
                            .map(str::to_owned);
                        return self
                            .send(
                                id,
                                GatewayEvent::SourceEvent {
                                    family,
                                    source_code,
                                    data,
                                },
                            )
                            .await;
                    }
                    return Ok(());
                }
                let ack = if !lot
                    && family == "instant_message"
                    && data.get("from_type").and_then(serde_json::Value::as_u64) == Some(5)
                {
                    Some((
                        data["from"].as_u64().unwrap_or(0) as u32,
                        data["ack_id"].as_str().unwrap_or("").to_owned(),
                    ))
                } else {
                    None
                };
                self.send(
                    None,
                    GatewayEvent::SourceEvent {
                        family,
                        source_code,
                        data,
                    },
                )
                .await?;
                if let Some((target, ack)) = ack
                    && let Some(city) = self.city.as_mut()
                {
                    native::write_packet(
                        &mut city.writer,
                        &instant_message_ack(self.projection.avatar_id.unwrap_or(0), target, &ack)?,
                    )
                    .await?;
                }
                Ok(())
            }
            SourcePacket::VmFrame { direct, data } if lot => {
                let old_plugin = self.eod.active_plugin();
                match self.eod.observe_for_lot(
                    direct,
                    &data,
                    self.projection
                        .lot_location
                        .ok_or_else(native::disconnected)?,
                ) {
                    Ok(events) => {
                        if self.projection.state == SessionState::LotConnecting {
                            self.projection.state = SessionState::LotReady;
                            self.update_session().await?;
                            if let Some(id) = self.complete("find_lot") {
                                self.outcome(id,OutcomeStatus::Accepted,Some(0),serde_json::json!({"lot_location":self.projection.lot_location,"admission":"lot_command_stream"}),None).await?;
                            }
                        }
                        if old_plugin != self.eod.active_plugin() {
                            self.update_session().await?;
                        }
                        for event in events {
                            let mut data = serde_json::to_value(event).map_err(|_| {
                                ServiceError::new(
                                    ErrorCode::InvalidResponse,
                                    "Cannot encode observed EOD event",
                                )
                            })?;
                            data["lot_incarnation"] =
                                serde_json::json!(self.projection.lot_incarnation);
                            self.send(
                                None,
                                GatewayEvent::SourceEvent {
                                    family: "eod".into(),
                                    source_code: None,
                                    data,
                                },
                            )
                            .await?;
                        }
                        for mut event in std::mem::take(&mut self.eod.source_events) {
                            event["lot_incarnation"] =
                                serde_json::json!(self.projection.lot_incarnation);
                            self.send(
                                None,
                                GatewayEvent::SourceEvent {
                                    family: "set_outfit".into(),
                                    source_code: None,
                                    data: event,
                                },
                            )
                            .await?;
                        }
                        if self.eod.state_sync
                            && let Some(id) = self.complete("world_snapshot")
                        {
                            self.outcome(id,OutcomeStatus::Accepted,None,serde_json::json!({"snapshot_received":true,"lot_incarnation":self.projection.lot_incarnation}),None).await?;
                        }
                    }
                    Err(error) => {
                        if old_plugin.is_some() {
                            self.update_session().await?;
                        }
                        self.send(None,GatewayEvent::SourceEvent{family:"vm_decode_error".into(),source_code:None,data:serde_json::json!({"error":error,"lot_incarnation":self.projection.lot_incarnation})}).await?;
                    }
                }
                self.send(
                    None,
                    GatewayEvent::VmFrame {
                        lot_incarnation: self.projection.lot_incarnation.unwrap_or(0),
                        direct,
                        data,
                    },
                )
                .await
            }
            SourcePacket::Unhandled { .. } => Ok(()),
            _ => Err(ServiceError::new(
                ErrorCode::InvalidResponse,
                "Unexpected packet for the active original session",
            )),
        }
    }
}

fn spawn_peer(
    connection: native::NativeConnection,
    generation: u64,
    lot: bool,
    tx: mpsc::Sender<NativeEvent>,
) -> Peer {
    let (mut reader, writer) = connection.stream.into_split();
    let task = tokio::spawn(async move {
        for packet in connection.queued {
            if tx
                .send(NativeEvent {
                    generation,
                    lot,
                    packet: Ok(packet),
                })
                .await
                .is_err()
            {
                return;
            }
        }
        loop {
            match native::read_packets(&mut reader).await {
                Ok(packets) => {
                    for packet in packets {
                        if tx
                            .send(NativeEvent {
                                generation,
                                lot,
                                packet: Ok(packet),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = tx
                        .send(NativeEvent {
                            generation,
                            lot,
                            packet: Err(error),
                        })
                        .await;
                    return;
                }
            }
        }
    });
    Peer {
        writer,
        reader: task,
    }
}
