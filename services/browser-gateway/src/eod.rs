use serde::Serialize;
use wonderland_game_services::{ErrorCode, ServiceError, ServiceResult};

#[derive(Clone, Debug, Serialize)]
pub struct ObservedEod {
    pub actor_uid: u32,
    pub plugin_id: u32,
    pub event_name: String,
    pub text: Option<String>,
    pub binary: Option<Vec<u8>>,
    pub incarnation: u64,
}
pub struct EodGate {
    actor: u32,
    active: Option<(u32, u64)>,
    incarnation: u64,
    last_tick: Option<u32>,
    pub source_events: Vec<serde_json::Value>,
    pub state_sync: bool,
}
impl EodGate {
    pub fn new(actor: u32) -> Self {
        Self {
            actor,
            active: None,
            incarnation: 0,
            last_tick: None,
            source_events: Vec::new(),
            state_sync: false,
        }
    }
    pub fn last_tick(&self) -> Option<u32> {
        self.last_tick
    }
    pub fn reset(&mut self, actor: u32) {
        self.actor = actor;
        self.active = None;
        self.last_tick = None;
        self.source_events.clear();
        self.state_sync = false;
    }
    pub fn active_plugin(&self) -> Option<u32> {
        self.active.map(|a| a.0)
    }
    pub fn authorize(&self, incarnation: u64, plugin: u32) -> ServiceResult<()> {
        if self.actor == 0 || self.active != Some((plugin, incarnation)) {
            return Err(ServiceError::new(
                ErrorCode::Unauthorized,
                "The operation does not match the active server-observed EOD session",
            ));
        }
        Ok(())
    }
    pub fn observe(&mut self, direct: bool, data: &[u8]) -> ServiceResult<Vec<ObservedEod>> {
        self.observe_inner(direct, data, None)
    }
    pub fn observe_for_lot(
        &mut self,
        direct: bool,
        data: &[u8],
        lot_location: u32,
    ) -> ServiceResult<Vec<ObservedEod>> {
        self.observe_inner(direct, data, Some(lot_location))
    }
    fn observe_inner(
        &mut self,
        direct: bool,
        data: &[u8],
        lot_location: Option<u32>,
    ) -> ServiceResult<Vec<ObservedEod>> {
        use wonderland_vm_protocol::{
            CommandBody, DecodeLimits, decode_direct_command, decode_tick_list,
        };
        let limits = DecodeLimits {
            max_input_bytes: 4 * 1024 * 1024,
            ..DecodeLimits::default()
        };
        self.source_events.clear();
        self.state_sync = false;
        let decoded = if direct {
            decode_direct_command(data, &limits).map(|command| vec![(None, vec![command])])
        } else {
            decode_tick_list(data, &limits).map(|list| {
                let immediate = list.immediate_mode;
                list.ticks
                    .into_iter()
                    .map(|tick| {
                        (
                            if immediate { None } else { Some(tick.tick_id) },
                            tick.commands,
                        )
                    })
                    .collect()
            })
        };
        let ticks = match decoded {
            Ok(ticks) => ticks,
            Err(_) => {
                self.active = None;
                return Err(ServiceError::new(
                    ErrorCode::InvalidResponse,
                    "Original VM packet could not be decoded atomically; EOD authority was cleared",
                ));
            }
        };
        if let Some(location)=lot_location
            && ticks.iter().flat_map(|(_,commands)|commands).any(|command|matches!(&command.body,CommandBody::StateSync{snapshot,..} if snapshot.platform.lot_id!=location)) {
            self.active=None;
            return Err(ServiceError::new(ErrorCode::InvalidResponse,"Original world snapshot belongs to a different admitted lot"));
        }
        let mut events = Vec::new();
        for (tick_id, commands) in ticks {
            let fresh = tick_id.is_none_or(|tick| {
                !self.last_tick.is_some_and(|last| {
                    let distance = tick.wrapping_sub(last);
                    distance == 0 || distance > 0x7fffffff
                })
            });
            if fresh && let Some(tick) = tick_id {
                self.last_tick = Some(tick);
            }
            for command in commands {
                match command.body {
                    CommandBody::StateSync { .. } => {
                        // Source SendState can deliver a cached/async older LastSync
                        // after live ticks, then replay history. Snapshot receipt is
                        // separate from ordinary tick freshness and EOD authority.
                        self.active = None;
                        self.state_sync = true;
                    }
                    _ if !fresh => {}
                    CommandBody::SourceFields { bytes } if command.kind == 33 => {
                        let mut complete = vec![33];
                        complete.extend(bytes);
                        if let Ok((
                            actor_uid,
                            wonderland_player_authoring::SourceCommand::SetOutfitServer {
                                avatar_id,
                                scope,
                                asset_id,
                            },
                            _,
                        )) = wonderland_player_authoring::decode_command_prefix(&complete)
                            && avatar_id == self.actor
                        {
                            self.source_events.push(serde_json::json!({"actor_uid":actor_uid,"uid":avatar_id,"scope":scope,"asset_id":asset_id.to_string()}));
                        }
                    }
                    CommandBody::EodMessage(message)
                        if message.actor_uid == self.actor && self.actor != 0 =>
                    {
                        use wonderland_vm_protocol::EodPayload;
                        let (text, binary) = match message.payload {
                            EodPayload::Text(text) => (Some(text), None),
                            EodPayload::Binary(binary) => (None, Some(binary)),
                        };
                        if message.event_name == "eod_enter" {
                            if binary.is_some() {
                                continue;
                            }
                            self.incarnation =
                                self.incarnation.checked_add(1).ok_or_else(|| {
                                    ServiceError::new(
                                        ErrorCode::ResourceBusy,
                                        "EOD generation space is exhausted; reconnect",
                                    )
                                })?;
                            self.active = Some((message.plugin_id, self.incarnation));
                        } else if self.active_plugin() != Some(message.plugin_id) {
                            continue;
                        }
                        let incarnation = self.active.map(|a| a.1).unwrap_or(self.incarnation);
                        if message.event_name == "eod_leave" && binary.is_none() {
                            self.active = None;
                        }
                        events.push(ObservedEod {
                            actor_uid: message.actor_uid,
                            plugin_id: message.plugin_id,
                            event_name: message.event_name,
                            text,
                            binary,
                            incarnation,
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(events)
    }
}
