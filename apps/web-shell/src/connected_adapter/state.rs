use super::*;
use wonderland_game_services::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Profile,
    People,
    Bookmarks,
    Chat,
    Inbox,
    Property,
    Neighborhood,
    Wardrobe,
    Eods,
    Settings,
    Create,
}

impl Panel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::People => "People",
            Self::Bookmarks => "Bookmarks",
            Self::Chat => "Chat",
            Self::Inbox => "Inbox",
            Self::Property => "Property",
            Self::Neighborhood => "Neighborhood",
            Self::Wardrobe => "Wardrobe",
            Self::Eods => "Object dialogs",
            Self::Settings => "Options",
            Self::Create => "Create a Sim",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::People | Self::Bookmarks => "users",
            Self::Property => "home",
            Self::Neighborhood => "building-community",
            Self::Wardrobe => "hanger",
            Self::Settings => "settings",
            Self::Create => "plus",
            Self::Eods => "device-gamepad-2",
            _ => "users",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum LoadState {
    #[default]
    Idle,
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct ReadSlot {
    pub status: LoadState,
    pub result: Option<DirectoryResult>,
}

#[derive(Clone, Debug)]
pub struct ServiceEvent {
    pub family: String,
    pub data: Value,
    pub operation_id: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct HomeIntent {
    browser_epoch: u64,
    source_epoch: u64,
    city_operation_id: String,
    avatar_id: u32,
    shard_name: String,
    lot_id: u32,
    lot_location: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ConnectedState {
    pub ledger: BrowserLedger,
    pub session: Option<SessionProjection>,
    pub health: Option<GatewayHealth>,
    pub roster: Vec<RosterEntry>,
    pub shards: Vec<Shard>,
    pub selected_avatar: Option<u32>,
    pub selected_shard: Option<String>,
    pub selected_neighborhood: Option<u32>,
    pub selected_bulletin: Option<u32>,
    pub selected_mail: Option<i32>,
    pub panel: Option<Panel>,
    pub login_busy: bool,
    pub socket_connecting: bool,
    pub notice: String,
    pub reads: BTreeMap<String, ReadSlot>,
    pub sent: BTreeMap<String, GatewayOperation>,
    pub wire_epochs: BTreeMap<String, u64>,
    pub results: BTreeMap<String, Value>,
    pub events: Vec<ServiceEvent>,
    pub lot_chat: lot_chat::LotChatLedger,
    pub chat_lot: bool,
    pub inbox: BTreeMap<i32, Value>,
    pub read_mail: BTreeSet<i32>,
    pub(crate) private_avatar: Option<u32>,
    pub roommate_invitations: BTreeMap<u32, Value>,
    pub(crate) home_intent: Option<HomeIntent>,
}

impl ConnectedState {
    pub fn accept_account(
        &mut self,
        epoch: u64,
        session: SessionProjection,
        roster: Vec<RosterEntry>,
        shards: Vec<Shard>,
    ) -> bool {
        if !self.ledger.accept_login(epoch) {
            return false;
        }
        self.selected_avatar = roster.first().map(|entry| entry.avatar_id);
        self.selected_shard = roster
            .first()
            .map(|entry| entry.shard_name.clone())
            .or_else(|| shards.first().map(|shard| shard.name.clone()));
        self.private_avatar = session.avatar_id.filter(|id| *id != 0);
        self.lot_chat.reset();
        self.session = Some(session);
        self.roster = roster;
        self.shards = shards;
        self.login_busy = false;
        self.notice.clear();
        true
    }

    pub fn receive(&mut self, local_epoch: u64, envelope: GatewayEnvelope) -> bool {
        if local_epoch != self.ledger.epoch || !self.ledger.authenticated {
            return false;
        }
        let Some(current_epoch) = self.session.as_ref().map(|s| s.epoch) else {
            return false;
        };
        if envelope.epoch != current_epoch
            && !(matches!(envelope.event, GatewayEvent::Session { .. })
                && envelope.epoch > current_epoch)
        {
            return false;
        }
        match envelope.event {
            GatewayEvent::Session { session } => {
                if session.epoch != envelope.epoch {
                    return false;
                }
                if self.property_admission_ready(&session) {
                    self.panel = None;
                }
                if self.home_intent.as_ref().is_some_and(|intent| {
                    !self.home_intent_matches(intent)
                        || session.epoch != intent.source_epoch
                        || !matches!(
                            session.state,
                            SessionState::CityConnecting | SessionState::CityReady
                        )
                        || session.avatar_id != Some(intent.avatar_id)
                        || session.shard_name.as_deref() != Some(intent.shard_name.as_str())
                }) {
                    self.cancel_home_intent();
                }
                if self.session.as_ref().is_some_and(|old| {
                    old.epoch != session.epoch
                        || old.avatar_id != session.avatar_id
                        || old.lot_incarnation != session.lot_incarnation
                        || (old.state == SessionState::LotReady
                            && session.state != SessionState::LotReady)
                }) || session.state == SessionState::Disconnected
                {
                    self.clear_live_hud();
                }
                if self.session.as_ref().is_some_and(|old| {
                    old.epoch != session.epoch
                        || old.avatar_id != session.avatar_id
                        || old.lot_incarnation != session.lot_incarnation
                        || old.lot_location != session.lot_location
                        || (old.state == SessionState::LotReady
                            && session.state != SessionState::LotReady)
                }) || session.state == SessionState::Disconnected
                {
                    self.lot_chat.reset();
                }
                if session.epoch > current_epoch || session.state == SessionState::Disconnected {
                    self.ledger.close_transport(
                        local_epoch,
                        "The game session changed before this action was confirmed.",
                    );
                }
                if let Some(avatar_id) = session.avatar_id.filter(|id| *id != 0) {
                    if self
                        .private_avatar
                        .is_some_and(|previous| previous != avatar_id)
                    {
                        self.ledger.drafts.clear();
                        self.ledger.unread.clear();
                        self.ledger.selected_person = None;
                        self.ledger.received_messages.clear();
                        self.events.clear();
                        self.inbox.clear();
                        self.read_mail.clear();
                        self.roommate_invitations.clear();
                        self.selected_mail = None;
                        // The newly submitted city admission is still awaiting its source result.
                        self.sent.retain(|_,operation|matches!(operation,GatewayOperation::ConnectCity {avatar_id:requested,..} if *requested==avatar_id));
                        self.ledger
                            .operations
                            .retain(|id, _| self.sent.contains_key(id));
                        self.results.retain(|id, _| self.sent.contains_key(id));
                        self.wire_epochs.retain(|id, _| self.sent.contains_key(id));
                    }
                    self.private_avatar = Some(avatar_id);
                    self.selected_avatar = Some(avatar_id);
                }
                if let Some(shard) = &session.shard_name {
                    self.selected_shard = Some(shard.clone());
                }
                let disconnected = session.state == SessionState::Disconnected;
                self.session = Some(session);
                if !disconnected {
                    self.ledger.open_transport(local_epoch);
                }
                self.socket_connecting = false;
                true
            }
            GatewayEvent::Pending { family } => {
                let Some(id) = envelope.operation_id else {
                    return false;
                };
                self.ledger.receive_operation(
                    &RequestStamp {
                        epoch: local_epoch,
                        operation_id: id,
                    },
                    OperationStatus::Pending(format!("Waiting for {family}…")),
                )
            }
            GatewayEvent::Outcome {
                status,
                source_code,
                data,
                error,
            } => {
                let Some(id) = envelope.operation_id else {
                    return false;
                };
                let message = error.map(|e| e.message).unwrap_or_else(|| match status {
                    OutcomeStatus::Accepted => "Confirmed by the server.".into(),
                    OutcomeStatus::Rejected => source_code
                        .map(|code| format!("The server declined this action (code {code})."))
                        .unwrap_or_else(|| "The server declined this action.".into()),
                    OutcomeStatus::Unknown => {
                        "The server has not confirmed this action. Refresh before trying it again."
                            .into()
                    }
                });
                let outcome = match status {
                    OutcomeStatus::Accepted => OperationStatus::Accepted(message),
                    OutcomeStatus::Rejected => OperationStatus::Rejected(message),
                    OutcomeStatus::Unknown => OperationStatus::Unknown(message),
                };
                if !self.ledger.receive_operation(
                    &RequestStamp {
                        epoch: local_epoch,
                        operation_id: id.clone(),
                    },
                    outcome,
                ) {
                    return false;
                }
                if status == OutcomeStatus::Accepted {
                    if let Some(GatewayOperation::MailPoll { since_ticks }) = self.sent.get(&id) {
                        let replace = since_ticks.0 == 0;
                        self.receive_mail(&data, replace);
                    }
                    if let Some(GatewayOperation::Roommate {
                        action: RoommateAction::Accept | RoommateAction::Decline,
                        lot_location,
                        ..
                    }) = self.sent.get(&id)
                    {
                        self.roommate_invitations.remove(lot_location);
                    }
                }
                if status != OutcomeStatus::Accepted
                    && let Some(operation) = self
                        .ledger
                        .operations
                        .get(&id)
                        .filter(|operation| operation.label == "Go home")
                {
                    self.notice = match &operation.status {
                        OperationStatus::Rejected(message) | OperationStatus::Unknown(message) => {
                            message.clone()
                        }
                        _ => String::new(),
                    };
                }
                self.results.insert(id, data);
                true
            }
            GatewayEvent::Roster { roster } => {
                if !roster
                    .iter()
                    .any(|entry| Some(entry.avatar_id) == self.selected_avatar)
                {
                    self.selected_avatar = roster.first().map(|entry| entry.avatar_id);
                }
                self.roster = roster;
                true
            }
            GatewayEvent::Directory { result } => {
                let slot = self
                    .reads
                    .iter()
                    .find(|(_, slot)| {
                        slot.result
                            .as_ref()
                            .is_some_and(|old| old.query == result.query)
                    })
                    .map(|(name, _)| name.clone());
                if let Some(slot) = slot {
                    self.reads.insert(
                        slot,
                        ReadSlot {
                            status: LoadState::Ready,
                            result: Some(result),
                        },
                    );
                }
                true
            }
            GatewayEvent::SourceEvent { family, data, .. } => {
                if family == "lot_chat" {
                    if !self.ledger.transport_ready {
                        return false;
                    }
                    let Ok(delivery) = serde_json::from_value(data) else {
                        return false;
                    };
                    let Some(session) = self.session.as_ref() else {
                        return false;
                    };
                    return self.lot_chat.receive(
                        session,
                        delivery,
                        self.panel == Some(Panel::Chat) && self.chat_lot,
                    );
                }
                if family == "instant_message" && source_u32(&data, "type") == Some(0) {
                    let Some(person) = source_u32(&data, "from") else {
                        return false;
                    };
                    let ack = source_text(&data, "ack_id");
                    let message_id = if ack.is_empty() {
                        format!("uncorrelated:{}", self.events.len())
                    } else {
                        format!("{person}:{ack}")
                    };
                    let visible = self.panel == Some(Panel::Chat)
                        && !self.chat_lot
                        && self.ledger.selected_person == Some(person);
                    if !self
                        .ledger
                        .receive_message(local_epoch, &message_id, person, visible)
                    {
                        return false;
                    }
                }
                if family == "mail" {
                    self.receive_mail(&data, false);
                }
                if family == "roommate_invitation"
                    && source_text(&data, "action") == "invite"
                    && let (Some(location), Some(_)) = (
                        source_u32(&data, "lot_location"),
                        source_u32(&data, "avatar_id"),
                    )
                {
                    self.roommate_invitations.insert(location, data.clone());
                }
                self.events.push(ServiceEvent {
                    family,
                    data,
                    operation_id: envelope.operation_id,
                });
                true
            }
            GatewayEvent::Error { error } => {
                if let Some(id) = envelope.operation_id {
                    self.ledger.receive_operation(
                        &RequestStamp {
                            epoch: local_epoch,
                            operation_id: id,
                        },
                        OperationStatus::Rejected(error.message.clone()),
                    );
                }
                self.notice = error.message;
                true
            }
            GatewayEvent::VmFrame { .. } => true,
        }
    }

    pub fn home_request(&self) -> Result<GatewayOperation, String> {
        if !self.ledger.authenticated || !self.ledger.transport_ready {
            return Err("Reconnect before going home.".into());
        }
        if self.has_home_intent()
            || self.busy("Go home city")
            || self.busy("Go home")
            || self.busy("Enter city")
        {
            return Err("Finish the current admission before going home.".into());
        }
        let entry = self.selected_entry().ok_or("Choose a Sim first.")?;
        let home = entry
            .home
            .as_ref()
            .filter(|home| home.lot_id != 0 && home.location != 0)
            .ok_or("This Sim has no home listed by the world.")?;
        let session = self
            .session
            .as_ref()
            .ok_or("Reconnect before going home.")?;
        match session.state {
            SessionState::CityReady
                if session.avatar_id == Some(entry.avatar_id)
                    && session.shard_name.as_deref() == Some(entry.shard_name.as_str()) =>
            {
                Ok(GatewayOperation::JoinLot {
                    lot_location: home.location,
                    open_if_closed: false,
                })
            }
            SessionState::Authenticated => Ok(GatewayOperation::ConnectCity {
                shard_name: entry.shard_name.clone(),
                avatar_id: entry.avatar_id,
            }),
            _ => Err("Finish or leave the current city or property before going home.".into()),
        }
    }

    fn property_admission_ready(&self, next: &SessionProjection) -> bool {
        let Some(previous) = &self.session else {
            return false;
        };
        if self.panel != Some(Panel::Property)
            || next.state != SessionState::LotReady
            || previous.state != SessionState::LotConnecting
            || previous.epoch != next.epoch
            || previous.avatar_id != next.avatar_id
            || previous.shard_name != next.shard_name
            || next.avatar_id.is_none_or(|id| id == 0)
            || next.lot_incarnation.is_none_or(|id| id == 0)
            || previous
                .lot_location
                .is_some_and(|location| next.lot_location != Some(location))
            || previous
                .lot_incarnation
                .is_some_and(|incarnation| next.lot_incarnation != Some(incarnation))
        {
            return false;
        }
        let property = self.data("property");
        let Some(location) = next.lot_location.filter(|location| *location != 0) else {
            return false;
        };
        if source_u32(&property, "location") != Some(location)
            || self.ledger.selected_lot.is_none()
            || source_u32(&property, "lot_id") != self.ledger.selected_lot
        {
            return false;
        }
        self.sent.iter().filter(|(_, operation)| matches!(operation, GatewayOperation::JoinLot { lot_location, .. } if *lot_location == location))
            .filter_map(|(id, _)| self.ledger.operations.get(id))
            .max_by_key(|operation| operation.stamp.operation_id.strip_prefix("ui-").and_then(|id| id.parse::<u64>().ok()).unwrap_or(0))
            .is_some_and(|operation| {
                operation.stamp.epoch == self.ledger.epoch
                    && self.wire_epochs.get(&operation.stamp.operation_id) == Some(&next.epoch)
                    && matches!(operation.label.as_str(), "Enter property" | "Open property")
                    && matches!(operation.status, OperationStatus::Waiting | OperationStatus::Pending(_) | OperationStatus::Accepted(_))
            })
    }

    pub fn select_private_conversation(&mut self, person: u32) {
        self.chat_lot = false;
        self.ledger.select_conversation(person);
        self.panel = Some(Panel::Chat);
    }

    /// Arm only the explicit request that was written by the Go home control.
    pub fn arm_home_city(&mut self, stamp: &RequestStamp) -> bool {
        self.cancel_home_intent();
        let Some(entry) = self.selected_entry().cloned() else {
            return false;
        };
        let Some(home) = entry
            .home
            .filter(|home| home.lot_id != 0 && home.location != 0)
        else {
            return false;
        };
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        if stamp.epoch != self.ledger.epoch
            || !self.ledger.authenticated
            || !self.ledger.transport_ready
            || session.state != SessionState::Authenticated
            || self.wire_epochs.get(&stamp.operation_id) != Some(&session.epoch)
            || !matches!(self.sent.get(&stamp.operation_id),Some(GatewayOperation::ConnectCity {avatar_id,shard_name}) if *avatar_id==entry.avatar_id&&*shard_name==entry.shard_name)
            || !self
                .ledger
                .operations
                .get(&stamp.operation_id)
                .is_some_and(|operation| {
                    operation.stamp == *stamp
                        && matches!(
                            operation.status,
                            OperationStatus::Waiting | OperationStatus::Pending(_)
                        )
                })
        {
            return false;
        }
        self.home_intent = Some(HomeIntent {
            browser_epoch: stamp.epoch,
            source_epoch: session.epoch,
            city_operation_id: stamp.operation_id.clone(),
            avatar_id: entry.avatar_id,
            shard_name: entry.shard_name,
            lot_id: home.lot_id,
            lot_location: home.location,
        });
        true
    }

    fn home_intent_matches(&self, intent: &HomeIntent) -> bool {
        if !self.ledger.authenticated
            || !self.ledger.transport_ready
            || self.ledger.epoch != intent.browser_epoch
            || self.selected_avatar != Some(intent.avatar_id)
            || self.selected_shard.as_deref() != Some(intent.shard_name.as_str())
        {
            return false;
        }
        let Some(entry) = self.selected_entry() else {
            return false;
        };
        if entry.shard_name != intent.shard_name
            || !entry.home.as_ref().is_some_and(|home| {
                home.lot_id == intent.lot_id && home.location == intent.lot_location
            })
        {
            return false;
        }
        let Some(session) = self
            .session
            .as_ref()
            .filter(|session| session.epoch == intent.source_epoch)
        else {
            return false;
        };
        match session.state {
            SessionState::Authenticated => true,
            SessionState::CityConnecting | SessionState::CityReady => {
                session.avatar_id == Some(intent.avatar_id)
                    && session.shard_name.as_deref() == Some(intent.shard_name.as_str())
            }
            _ => false,
        }
    }

    /// Consume once, only after both the correlated receipt and actual city readiness.
    pub fn take_home_follow_up(&mut self) -> Option<GatewayOperation> {
        let intent = self.home_intent.clone()?;
        if !self.home_intent_matches(&intent)
            || self.wire_epochs.get(&intent.city_operation_id) != Some(&intent.source_epoch)
            || !matches!(self.sent.get(&intent.city_operation_id),Some(GatewayOperation::ConnectCity {avatar_id,shard_name}) if *avatar_id==intent.avatar_id&&*shard_name==intent.shard_name)
        {
            self.cancel_home_intent();
            return None;
        }
        match self
            .ledger
            .operations
            .get(&intent.city_operation_id)
            .map(|operation| &operation.status)
        {
            Some(OperationStatus::Accepted(_)) => {
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.state == SessionState::CityReady)
                {
                    self.cancel_home_intent();
                    Some(GatewayOperation::JoinLot {
                        lot_location: intent.lot_location,
                        open_if_closed: false,
                    })
                } else {
                    None
                }
            }
            Some(OperationStatus::Waiting | OperationStatus::Pending(_)) => None,
            _ => {
                self.cancel_home_intent();
                None
            }
        }
    }

    pub fn cancel_home_intent(&mut self) {
        self.home_intent = None;
    }
    pub fn has_home_intent(&self) -> bool {
        self.home_intent.is_some()
    }

    pub fn clear_live_hud(&mut self) {
        self.lot_chat.suspend();
        for entry in &mut self.roster {
            entry.money = None;
            entry.motives = None;
        }
    }

    pub fn selected_entry(&self) -> Option<&RosterEntry> {
        self.roster
            .iter()
            .find(|entry| Some(entry.avatar_id) == self.selected_avatar)
    }

    pub fn active_entry(&self) -> Option<&RosterEntry> {
        let avatar = self.session.as_ref()?.avatar_id?;
        self.roster.iter().find(|entry| entry.avatar_id == avatar)
    }

    pub fn shard(&self) -> Option<&Shard> {
        self.shards
            .iter()
            .find(|shard| Some(&shard.name) == self.selected_shard.as_ref())
    }

    pub fn in_city(&self) -> bool {
        self.session.as_ref().is_some_and(|s| {
            matches!(
                s.state,
                SessionState::CityReady | SessionState::LotConnecting | SessionState::LotReady
            )
        })
    }

    pub fn capability_reason(&self, name: &str) -> Option<String> {
        let capability = self
            .session
            .as_ref()
            .and_then(|s| s.capabilities.iter().find(|c| c.capability == name))
            .or_else(|| {
                self.health
                    .as_ref()
                    .and_then(|h| h.capabilities.iter().find(|c| c.capability == name))
            });
        match capability {
            Some(status) if status.available => None,
            Some(status) => Some(
                status
                    .reason
                    .clone()
                    .unwrap_or_else(|| "This action is not available in this session.".into()),
            ),
            None => Some("This service is not available in this session.".into()),
        }
    }

    pub fn read(&self, slot: &str) -> ReadSlot {
        self.reads.get(slot).cloned().unwrap_or_default()
    }

    pub fn data(&self, slot: &str) -> Value {
        self.reads
            .get(slot)
            .and_then(|slot| slot.result.as_ref())
            .map(|result| result.data.clone())
            .unwrap_or(Value::Null)
    }

    pub fn page(&self, slot: &str) -> DirectoryPage {
        self.reads
            .get(slot)
            .and_then(|slot| slot.result.as_ref())
            .map(DirectoryPage::from_result)
            .unwrap_or_default()
    }

    pub fn draft(&self, key: &str) -> String {
        self.ledger.drafts.get(key).cloned().unwrap_or_default()
    }

    pub fn busy(&self, label: &str) -> bool {
        self.ledger.operations.values().any(|operation| {
            operation.label == label
                && matches!(
                    operation.status,
                    OperationStatus::Waiting | OperationStatus::Pending(_)
                )
        })
    }

    pub fn last_operation(&self, label: &str) -> Option<PendingOperation> {
        self.ledger
            .operations
            .values()
            .filter(|operation| operation.label == label)
            .max_by_key(|operation| {
                operation
                    .stamp
                    .operation_id
                    .strip_prefix("ui-")
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(0)
            })
            .cloned()
    }

    fn receive_mail(&mut self, data: &Value, replace: bool) {
        let Some(messages) = data.get("messages").and_then(Value::as_array) else {
            return;
        };
        if replace {
            self.inbox.clear();
        }
        for message in messages {
            if let Some(id) = message
                .get("id")
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
            {
                if source_u32(message, "read_state").is_some_and(|state| state != 0) {
                    self.read_mail.insert(id);
                }
                self.inbox.insert(id, message.clone());
            }
        }
        if self
            .selected_mail
            .is_some_and(|id| !self.inbox.contains_key(&id))
        {
            self.selected_mail = None;
        }
    }
}
