use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEodMessage {
    pub actor_uid: u32,
    pub plugin_id: u32,
    pub event_name: String,
    pub text: Option<String>,
    pub binary: Option<Vec<u8>>,
    pub incarnation: u64,
}
impl AuthoringState {
    /// Consume only complete source events already matched to the authenticated
    /// actor and current lot incarnation by the transport adapter. eod_enter is
    /// the sole way this observer admits a dresser/rack; no UI click creates one.
    pub fn observe_eod(
        &mut self,
        actor: SourceActorLot,
        message: &SourceEodMessage,
    ) -> Result<bool, AuthoringError> {
        if message.actor_uid != actor.avatar_id
            || message.incarnation == 0
            || ![DRESSER_PLUGIN, RACK_CUSTOMER_PLUGIN, RACK_OWNER_PLUGIN]
                .contains(&message.plugin_id)
        {
            return Ok(false);
        }
        if message.event_name == "eod_enter" {
            if self
                .snapshot
                .as_ref()
                .filter(|s| s.actor == actor)
                .and_then(|s| s.eod.as_ref())
                .is_some_and(|e| e.incarnation >= message.incarnation)
            {
                return Ok(false);
            }
            let mut snapshot = self
                .snapshot
                .as_ref()
                .filter(|s| s.actor == actor)
                .cloned()
                .unwrap_or_else(|| AuthoringSnapshot {
                    actor: actor.clone(),
                    revision: 0,
                    permission: None,
                    community: false,
                    bounds: None,
                    budget: None,
                    can_place_user: false,
                    can_place_donated: false,
                    build_resources: vec![],
                    outfits_loaded: false,
                    catalog: vec![],
                    inventory: vec![],
                    objects: vec![],
                    outfits: vec![],
                    defaults: Default::default(),
                    eod: None,
                });
            snapshot.revision = snapshot
                .revision
                .checked_add(1)
                .ok_or(AuthoringError::Invalid("source observation revision"))?;
            snapshot.outfits.clear();
            snapshot.outfits_loaded = false;
            snapshot.eod = Some(EodSession {
                incarnation: message.incarnation,
                plugin_id: message.plugin_id,
                actor_id: actor.avatar_id,
                object_pid: None,
                object_owner_id: None,
                rack_type: None,
            });
            if self
                .pending
                .as_ref()
                .is_some_and(|r| matches!(r.wire, AuthoringWire::Eod { .. }))
            {
                self.abandon_pending();
                self.status = OperationState::Unknown(
                    "The object dialog changed before this action was confirmed.".into(),
                );
            }
            self.install_observation(snapshot)?;
            return Ok(true);
        }
        let Some(current) = self.snapshot.as_ref().filter(|s| s.actor == actor) else {
            return Ok(false);
        };
        let Some(session) = current.eod.as_ref().filter(|e| {
            e.incarnation == message.incarnation
                && e.plugin_id == message.plugin_id
                && e.actor_id == message.actor_uid
        }) else {
            return Ok(false);
        };
        let mut snapshot = current.clone();
        let mut accepted = false;
        match message.event_name.as_str() {
            "set_outfits" => {
                let rows = decode_outfit_stock(
                    message
                        .binary
                        .as_deref()
                        .ok_or(AuthoringError::Missing("source stock bytes"))?,
                )?;
                if message.plugin_id == DRESSER_PLUGIN
                    && rows
                        .iter()
                        .any(|row| row.owner_type != 1 || row.owner_id != actor.avatar_id)
                {
                    return Err(AuthoringError::EodOwnership);
                }
                if message.plugin_id != DRESSER_PLUGIN
                    && rows.iter().any(|row| {
                        row.owner_type != 2
                            || session.object_pid.is_some_and(|pid| pid != row.owner_id)
                    })
                {
                    return Err(AuthoringError::EodOwnership);
                }
                if let Some(request) = &self.pending {
                    accepted = match request.intent {
                        AuthoringIntent::DeleteOutfit { outfit_id }
                        | AuthoringIntent::RackDelete { outfit_id } => {
                            current.outfits.iter().any(|r| r.outfit_id == outfit_id)
                                && !rows.iter().any(|r| r.outfit_id == outfit_id)
                        }
                        AuthoringIntent::RackSetPrice { outfit_id, price } => rows
                            .iter()
                            .any(|r| r.outfit_id == outfit_id && r.sale_price == price),
                        _ => false,
                    };
                }
                if message.plugin_id != DRESSER_PLUGIN
                    && let Some(first) = rows.first()
                {
                    if rows.iter().any(|row| row.owner_id != first.owner_id) {
                        return Err(AuthoringError::EodOwnership);
                    }
                    snapshot.eod.as_mut().unwrap().object_pid = Some(first.owner_id);
                    snapshot.eod.as_mut().unwrap().object_owner_id = snapshot
                        .objects
                        .iter()
                        .find(|o| o.persist_id == first.owner_id && !o.is_avatar)
                        .map(|o| o.owner_id);
                }
                snapshot.outfits = rows;
                snapshot.outfits_loaded = true;
            }
            "rack_buy_error" => {
                let reason = match message.binary.as_deref() {
                    Some([0]) => "You already own this outfit.",
                    Some([1]) => "You already own five outfits in this category.",
                    Some([2]) => "The purchase payment was rejected.",
                    _ => return Err(AuthoringError::Invalid("rack error code")),
                };
                if let Some(request) = self
                    .pending
                    .as_ref()
                    .filter(|r| matches!(r.intent, AuthoringIntent::RackBuy { .. }))
                {
                    self.receive(AuthoringReceipt::Rejected {
                        operation_id: request.operation_id,
                        actor,
                        message: reason.into(),
                    })?;
                }
                return Ok(true);
            }
            "eod_leave" => {
                accepted = self
                    .pending
                    .as_ref()
                    .is_some_and(|r| matches!(r.intent, AuthoringIntent::CloseEod));
                snapshot.eod = None;
                snapshot.outfits.clear();
                snapshot.outfits_loaded = false;
                if !accepted
                    && self
                        .pending
                        .as_ref()
                        .is_some_and(|r| matches!(r.wire, AuthoringWire::Eod { .. }))
                {
                    self.abandon_pending();
                    self.status = OperationState::Unknown(
                        "The object dialog closed before this action was confirmed.".into(),
                    );
                }
            }
            "rack_show" => {
                let rack_type = message
                    .text
                    .as_deref()
                    .and_then(|s| s.parse::<u8>().ok())
                    .filter(|t| *t <= 8)
                    .ok_or(AuthoringError::Invalid("source rack type"))?;
                snapshot.eod.as_mut().unwrap().rack_type = Some(rack_type);
            }
            "dresser_refresh_default" | "dresser_show" | "rack_initialize_name" => {
                return Ok(true);
            }
            _ => return Ok(false),
        }
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::Invalid("source observation revision"))?;
        self.install_observation(snapshot.clone())?;
        if accepted && let Some(request) = &self.pending {
            self.receive(AuthoringReceipt::Accepted {
                operation_id: request.operation_id,
                actor,
                snapshot: Box::new(snapshot),
                amount: None,
            })?;
        }
        Ok(true)
    }
    /// A real sequenced server SetOutfit can confirm wear/default; sending an EOD
    /// request or receiving a generic refresh is insufficient evidence.
    pub fn observe_set_outfit(
        &mut self,
        actor: &SourceActorLot,
        uid: u32,
        scope: i16,
        asset: u64,
    ) -> Result<bool, AuthoringError> {
        if uid != actor.avatar_id {
            return Ok(false);
        }
        let Some(current) = self.snapshot.as_ref().filter(|s| s.actor == *actor) else {
            return Ok(false);
        };
        if ![0, 2, 5, 8, 9, 10, 11, 22, 23, 24, 25].contains(&scope) {
            return Ok(false);
        }
        let mut snapshot = current.clone();
        let mut accepted = false;
        if let Some(request) = &self.pending
            && let Some(eod) = snapshot.eod.as_ref()
        {
            if matches!(request.wire,AuthoringWire::Eod{incarnation,plugin_id,..}if incarnation==eod.incarnation&&plugin_id==DRESSER_PLUGIN)
            {
                let selection = match request.intent {
                    AuthoringIntent::Wear { outfit_id } => Some((outfit_id, false)),
                    AuthoringIntent::SetDefault { outfit_id } => Some((outfit_id, true)),
                    _ => None,
                };
                if let Some((outfit_id, default)) = selection
                    && let Some(row) = snapshot
                        .outfits
                        .iter()
                        .find(|row| row.outfit_id == outfit_id)
                {
                    let expected = if default {
                        i16::from(row.category)
                    } else {
                        match row.category {
                            0 => 22,
                            2 => 23,
                            5 => 24,
                            c => i16::from(c),
                        }
                    };
                    accepted = scope == expected
                        && asset == row.asset_id
                        && row.owner_id == uid
                        && row.owner_type == 1;
                }
            }
            if matches!(request.wire,AuthoringWire::Eod{incarnation,plugin_id,..}if incarnation==eod.incarnation&&plugin_id==RACK_CUSTOMER_PLUGIN)
                && let AuthoringIntent::RackTry { outfit_id } = request.intent
                && let Some(row) = snapshot.outfits.iter().find(|r| {
                    r.outfit_id == outfit_id
                        && r.owner_type == 2
                        && eod.object_pid == Some(r.owner_id)
                })
            {
                let expected = match eod.rack_type {
                    Some(0 | 1 | 2 | 3 | 8) => Some(25),
                    Some(4) => Some(8),
                    Some(5) => Some(9),
                    Some(6) => Some(10),
                    Some(7) => Some(11),
                    _ => None,
                };
                accepted = expected == Some(scope) && asset == row.asset_id;
            }
        }
        if [0, 2, 5].contains(&scope) {
            snapshot.defaults.insert(scope as u8, asset.to_string());
        }
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::Invalid("source observation revision"))?;
        self.install_observation(snapshot.clone())?;
        if accepted && let Some(request) = &self.pending {
            self.receive(AuthoringReceipt::Accepted {
                operation_id: request.operation_id,
                actor: actor.clone(),
                snapshot: Box::new(snapshot),
                amount: None,
            })?;
        }
        Ok(true)
    }
}
