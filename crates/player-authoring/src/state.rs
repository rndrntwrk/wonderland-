use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const DRESSER_PLUGIN: u32 = 0x8b300068;
pub const RACK_CUSTOMER_PLUGIN: u32 = 0xcb492685;
pub const RACK_OWNER_PLUGIN: u32 = 0x2b58020b;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceActorLot {
    pub avatar_id: u32,
    pub lot_id: Option<u32>,
    pub location: u32,
    pub epoch: u64,
    pub incarnation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIdentity {
    pub object_id: i16,
    pub incarnation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceObject {
    pub entity: EntityIdentity,
    pub persist_id: u32,
    pub guid: u32,
    pub name: String,
    pub owner_id: u32,
    pub donated: bool,
    pub is_avatar: bool,
    /// Source IsUserMovable needs object content; FSOv alone leaves it unknown.
    pub movable: Option<bool>,
    pub transaction_incomplete: bool,
    pub placement: Option<Placement>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub persist_id: u32,
    pub owner_id: u32,
    pub guid: u32,
    pub name: String,
    pub value: u32,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodSession {
    pub incarnation: u64,
    pub plugin_id: u32,
    pub actor_id: u32,
    pub object_pid: Option<u32>,
    pub object_owner_id: Option<u32>,
    pub rack_type: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotBounds {
    pub width: u16,
    pub height: u16,
    pub levels: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildResource {
    pub tool: u8,
    pub pattern: u16,
    pub style: u16,
    pub name: String,
    pub price: Option<u32>,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthoringSnapshot {
    pub actor: SourceActorLot,
    pub revision: u64,
    pub permission: Option<u8>,
    pub community: bool,
    pub bounds: Option<LotBounds>,
    pub budget: Option<u32>,
    pub can_place_user: bool,
    pub can_place_donated: bool,
    pub build_resources: Vec<BuildResource>,
    pub outfits_loaded: bool,
    pub catalog: Vec<CatalogItem>,
    pub inventory: Vec<InventoryItem>,
    pub objects: Vec<SourceObject>,
    pub outfits: Vec<OwnedOutfit>,
    pub defaults: BTreeMap<u8, String>,
    pub eod: Option<EodSession>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AuthoringIntent {
    Wear {
        outfit_id: u32,
    },
    SetDefault {
        outfit_id: u32,
    },
    DeleteOutfit {
        outfit_id: u32,
    },
    RackTry {
        outfit_id: u32,
    },
    RackBuy {
        outfit_id: u32,
        wear_now: bool,
    },
    RackStock {
        #[serde(with = "crate::decimal_u64")]
        asset_id: u64,
    },
    RackSetPrice {
        outfit_id: u32,
        price: i32,
    },
    RackDelete {
        outfit_id: u32,
    },
    CloseEod,
    Buy {
        guid: u32,
        placement: Placement,
        desired_mode: u8,
        upgrade: u8,
    },
    PlaceInventory {
        persist_id: u32,
        placement: Placement,
        desired_mode: u8,
    },
    Move {
        entity: EntityIdentity,
        placement: Placement,
    },
    Delete {
        entity: EntityIdentity,
        desired_mode: u8,
    },
    SendToInventory {
        entity: EntityIdentity,
    },
    Architecture {
        commands: Vec<ArchitectureCommand>,
    },
    SetRoof {
        pitch: f32,
        style: u32,
    },
}
impl AuthoringIntent {
    pub fn is_world(&self) -> bool {
        matches!(
            self,
            Self::Buy { .. }
                | Self::PlaceInventory { .. }
                | Self::Move { .. }
                | Self::Delete { .. }
                | Self::SendToInventory { .. }
                | Self::Architecture { .. }
                | Self::SetRoof { .. }
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthoringWire {
    LotCommand {
        data: Vec<u8>,
    },
    Eod {
        incarnation: u64,
        plugin_id: u32,
        event_name: String,
        text: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthoringRequest {
    pub operation_id: u64,
    pub actor: SourceActorLot,
    pub revision: u64,
    pub intent: AuthoringIntent,
    pub wire: AuthoringWire,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OperationState {
    #[default]
    Idle,
    Pending,
    Unknown(String),
    Rejected(String),
    Accepted {
        amount: Option<i32>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AuthoringReceipt {
    Unknown {
        operation_id: u64,
        actor: SourceActorLot,
        message: String,
    },
    Rejected {
        operation_id: u64,
        actor: SourceActorLot,
        message: String,
    },
    Accepted {
        operation_id: u64,
        actor: SourceActorLot,
        snapshot: Box<AuthoringSnapshot>,
        amount: Option<i32>,
    },
}
#[derive(Clone, Debug, Default)]
pub struct AuthoringState {
    pub snapshot: Option<AuthoringSnapshot>,
    pub pending: Option<AuthoringRequest>,
    pub draft: Option<AuthoringIntent>,
    pub status: OperationState,
    next_operation: u64,
    /// Local refresh/pick generation, never a source VM command revision.
    pub presentation_generation: Option<u64>,
    pub world_projection_valid: bool,
    pub unknown_operations: Vec<AuthoringRequest>,
    pub diagonal_floor_tiles: Option<std::collections::BTreeSet<(u16, u16, u8)>>,
}
impl AuthoringSnapshot {
    pub fn validate(&self) -> Result<(), AuthoringError> {
        use std::collections::BTreeSet;
        if self.actor.avatar_id == 0
            || self.actor.incarnation == 0
            || self.permission.is_some_and(|p| p > 4)
        {
            return Err(AuthoringError::Invalid("source actor/permission"));
        }
        if self
            .bounds
            .as_ref()
            .is_some_and(|b| b.width == 0 || b.height == 0 || b.levels == 0)
        {
            return Err(AuthoringError::Invalid("lot bounds"));
        }
        let mut ids = BTreeSet::new();
        if self
            .outfits
            .iter()
            .any(|o| o.outfit_id == 0 || !ids.insert(o.outfit_id))
        {
            return Err(AuthoringError::Invalid("outfit identity"));
        }
        let mut ids = BTreeSet::new();
        if self.objects.iter().any(|o| {
            o.entity.object_id == 0 || o.entity.incarnation == 0 || !ids.insert(o.entity.object_id)
        }) {
            return Err(AuthoringError::Invalid("entity incarnation"));
        }
        let mut ids = BTreeSet::new();
        if self
            .inventory
            .iter()
            .any(|o| o.persist_id == 0 || !ids.insert(o.persist_id))
        {
            return Err(AuthoringError::Invalid("inventory identity"));
        }
        Ok(())
    }
    fn placement(&self, p: &Placement) -> Result<(), AuthoringError> {
        let bounds = self
            .bounds
            .as_ref()
            .ok_or(AuthoringError::Missing("source lot bounds"))?;
        if p.x < 0
            || p.y < 0
            || i32::from(p.x) >= i32::from(bounds.width) * 16
            || i32::from(p.y) >= i32::from(bounds.height) * 16
            || p.level < 1
            || p.level as u8 > bounds.levels
            || p.direction.count_ones() != 1
        {
            return Err(AuthoringError::Invalid("placement outside source lot"));
        }
        Ok(())
    }
    fn object(&self, entity: &EntityIdentity) -> Result<&SourceObject, AuthoringError> {
        let object = self
            .objects
            .iter()
            .find(|o| o.entity == *entity)
            .ok_or(AuthoringError::Stale)?;
        Ok(object)
    }
    fn eod(&self, plugin: u32) -> Result<&EodSession, AuthoringError> {
        let eod = self
            .eod
            .as_ref()
            .ok_or(AuthoringError::Missing("active object dialog"))?;
        if eod.plugin_id != plugin || eod.incarnation == 0 || eod.actor_id != self.actor.avatar_id {
            return Err(AuthoringError::EodOwnership);
        }
        Ok(eod)
    }
    fn owned_outfit(&self, id: u32) -> Result<&OwnedOutfit, AuthoringError> {
        let row = self
            .outfits
            .iter()
            .find(|o| o.outfit_id == id)
            .ok_or(AuthoringError::Missing("owned outfit"))?;
        if row.owner_type != 1 || row.owner_id != self.actor.avatar_id {
            return Err(AuthoringError::EodOwnership);
        }
        Ok(row)
    }
    fn rack_outfit(&self, id: u32, eod: &EodSession) -> Result<&OwnedOutfit, AuthoringError> {
        let row = self
            .outfits
            .iter()
            .find(|o| o.outfit_id == id)
            .ok_or(AuthoringError::Missing("rack outfit"))?;
        if row.owner_type != 2 || eod.object_pid.is_some_and(|pid| pid != row.owner_id) {
            return Err(AuthoringError::EodOwnership);
        }
        Ok(row)
    }
    fn rack_owner(&self) -> Result<&EodSession, AuthoringError> {
        let eod = self.eod(RACK_OWNER_PLUGIN)?;
        if eod.object_owner_id != Some(self.actor.avatar_id) {
            return Err(AuthoringError::EodOwnership);
        }
        Ok(eod)
    }
    pub fn prepare(&self, intent: &AuthoringIntent) -> Result<AuthoringWire, AuthoringError> {
        self.validate()?;
        let message = |eod: &EodSession, event: &str, text: String| AuthoringWire::Eod {
            incarnation: eod.incarnation,
            plugin_id: eod.plugin_id,
            event_name: event.into(),
            text,
        };
        let command = match intent {
            AuthoringIntent::Wear { outfit_id } => {
                let eod = self.eod(DRESSER_PLUGIN)?;
                let row = self.owned_outfit(*outfit_id)?;
                if ![0, 2, 5, 8, 9, 10, 11].contains(&row.category) {
                    return Err(AuthoringError::Invalid("dresser category"));
                }
                return Ok(message(eod, "dresser_change_outfit", outfit_id.to_string()));
            }
            AuthoringIntent::SetDefault { outfit_id } => {
                let eod = self.eod(DRESSER_PLUGIN)?;
                let row = self.owned_outfit(*outfit_id)?;
                if ![0, 2, 5].contains(&row.category) {
                    return Err(AuthoringError::Invalid("default clothing category"));
                }
                return Ok(message(
                    eod,
                    "dresser_set_default",
                    format!("{},{}", row.category, outfit_id),
                ));
            }
            AuthoringIntent::DeleteOutfit { outfit_id } => {
                let eod = self.eod(DRESSER_PLUGIN)?;
                let row = self.owned_outfit(*outfit_id)?;
                if [0, 2, 5].contains(&row.category)
                    && self
                        .outfits
                        .iter()
                        .filter(|o| {
                            o.owner_type == 1
                                && o.owner_id == self.actor.avatar_id
                                && o.category == row.category
                        })
                        .count()
                        <= 1
                {
                    return Err(AuthoringError::Permission);
                }
                if ![0, 2, 5, 8, 9, 10, 11].contains(&row.category) {
                    return Err(AuthoringError::Invalid("dresser category"));
                }
                return Ok(message(eod, "dresser_delete_outfit", outfit_id.to_string()));
            }
            AuthoringIntent::RackTry { outfit_id } => {
                let eod = self.eod(RACK_CUSTOMER_PLUGIN)?;
                if eod.rack_type.is_none() {
                    return Err(AuthoringError::Missing("source rack type"));
                }
                self.rack_outfit(*outfit_id, eod)?;
                return Ok(message(eod, "rack_try_outfit_on", outfit_id.to_string()));
            }
            AuthoringIntent::RackBuy {
                outfit_id,
                wear_now,
            } => {
                let eod = self.eod(RACK_CUSTOMER_PLUGIN)?;
                let row = self.rack_outfit(*outfit_id, eod)?;
                if self.outfits.iter().any(|o| {
                    o.owner_type == 1
                        && o.owner_id == self.actor.avatar_id
                        && o.asset_id == row.asset_id
                }) {
                    return Err(AuthoringError::Permission);
                }
                if row.sale_price < 0 {
                    return Err(AuthoringError::Price);
                }
                return Ok(message(
                    eod,
                    "rack_purchase",
                    format!("{},{}", outfit_id, wear_now),
                ));
            }
            AuthoringIntent::RackStock { asset_id } => {
                let eod = self.rack_owner()?;
                return Ok(message(eod, "rackowner_stock", asset_id.to_string()));
            }
            AuthoringIntent::RackSetPrice { outfit_id, price } => {
                let eod = self.rack_owner()?;
                self.rack_outfit(*outfit_id, eod)?;
                if !(1..=999999).contains(price) {
                    return Err(AuthoringError::Price);
                }
                return Ok(message(
                    eod,
                    "rackowner_update_price",
                    format!("{},{}", outfit_id, price),
                ));
            }
            AuthoringIntent::RackDelete { outfit_id } => {
                let eod = self.rack_owner()?;
                self.rack_outfit(*outfit_id, eod)?;
                return Ok(message(eod, "rackowner_delete", outfit_id.to_string()));
            }
            AuthoringIntent::CloseEod => {
                let eod = self
                    .eod
                    .as_ref()
                    .ok_or(AuthoringError::Missing("active object dialog"))?;
                if eod.actor_id != self.actor.avatar_id {
                    return Err(AuthoringError::EodOwnership);
                }
                return Ok(message(eod, "close", String::new()));
            }
            AuthoringIntent::Buy {
                guid,
                placement,
                desired_mode,
                upgrade,
            } => {
                self.placement(placement)?;
                let row = self
                    .catalog
                    .iter()
                    .find(|o| o.guid == *guid)
                    .ok_or(AuthoringError::Missing("source catalog item"))?;
                if *guid == 0x24C95F99 || row.disable_level > 2 {
                    return Err(AuthoringError::Permission);
                }
                let mode = source_purchase_mode(
                    self.permission
                        .ok_or(AuthoringError::Missing("lot permission"))?,
                    self.community,
                    row.category,
                    *desired_mode,
                    false,
                )?;
                if (mode == 1 && !self.can_place_user) || (mode == 2 && !self.can_place_donated) {
                    return Err(AuthoringError::Permission);
                }
                SourceCommand::Buy {
                    guid: *guid,
                    placement: placement.clone(),
                    value: -1,
                    mode,
                    upgrade: *upgrade,
                }
            }
            AuthoringIntent::PlaceInventory {
                persist_id,
                placement,
                desired_mode,
            } => {
                self.placement(placement)?;
                let row = self
                    .inventory
                    .iter()
                    .find(|o| o.persist_id == *persist_id)
                    .ok_or(AuthoringError::Missing("inventory item"))?;
                if row.owner_id != self.actor.avatar_id {
                    return Err(AuthoringError::Permission);
                }
                if self
                    .catalog
                    .iter()
                    .find(|o| o.guid == row.guid)
                    .is_some_and(|o| o.disable_level > 2)
                {
                    return Err(AuthoringError::Permission);
                }
                let mode = source_purchase_mode(
                    self.permission
                        .ok_or(AuthoringError::Missing("lot permission"))?,
                    self.community,
                    0,
                    *desired_mode,
                    true,
                )?;
                if (mode == 1 && !self.can_place_user) || (mode == 2 && !self.can_place_donated) {
                    return Err(AuthoringError::Permission);
                }
                SourceCommand::PlaceInventory {
                    persist_id: *persist_id,
                    placement: placement.clone(),
                    restore: Default::default(),
                    mode,
                }
            }
            AuthoringIntent::Move { entity, placement } => {
                self.placement(placement)?;
                let object = self.object(entity)?;
                if self.permission != Some(4) && object.movable.is_none() {
                    return Err(AuthoringError::Missing("source object movability"));
                }
                if self.permission.unwrap_or(0) < 1
                    || object.is_avatar
                    || (self.permission != Some(4) && object.movable != Some(true))
                {
                    return Err(AuthoringError::Permission);
                }
                SourceCommand::Move {
                    object_id: entity.object_id,
                    placement: placement.clone(),
                }
            }
            AuthoringIntent::Delete {
                entity,
                desired_mode,
            } => {
                let object = self.object(entity)?;
                if self.permission != Some(4) && object.movable.is_none() {
                    return Err(AuthoringError::Missing("source object movability"));
                }
                if self.permission != Some(4)
                    && (object.movable != Some(true) || object.transaction_incomplete)
                {
                    return Err(AuthoringError::Permission);
                }
                let category = self
                    .catalog
                    .iter()
                    .find(|row| row.guid == object.guid)
                    .map(|o| o.category);
                let mode = source_delete_mode(
                    self.actor.avatar_id,
                    self.permission
                        .ok_or(AuthoringError::Missing("lot permission"))?,
                    self.community,
                    object,
                    category,
                    *desired_mode,
                )?;
                SourceCommand::Delete {
                    object_id: entity.object_id,
                    persist_id: 0,
                    cleanup_all: true,
                    success: false,
                    mode,
                }
            }
            AuthoringIntent::SendToInventory { entity } => {
                let object = self.object(entity)?;
                let category = self
                    .catalog
                    .iter()
                    .find(|row| row.guid == object.guid)
                    .map(|o| o.category);
                if object.movable.is_none() {
                    return Err(AuthoringError::Missing("source object movability"));
                }
                if self.permission.unwrap_or(0) < 1
                    || object.movable != Some(true)
                    || object.transaction_incomplete
                    || source_delete_mode(
                        self.actor.avatar_id,
                        self.permission.unwrap_or(0),
                        self.community,
                        object,
                        category,
                        1,
                    )? != 1
                {
                    return Err(AuthoringError::Permission);
                }
                SourceCommand::SendToInventory {
                    persist_id: object.persist_id,
                    success: false,
                }
            }
            AuthoringIntent::Architecture { commands } => {
                if self.permission.unwrap_or(0) < 2 {
                    return Err(AuthoringError::Permission);
                }
                let b = self
                    .bounds
                    .as_ref()
                    .ok_or(AuthoringError::Missing("lot bounds"))?;
                if commands.is_empty()
                    || commands.iter().any(|c| {
                        c.kind > 9
                            || c.x < 0
                            || c.y < 0
                            || c.x >= i32::from(b.width)
                            || c.y >= i32::from(b.height)
                            || c.level < 1
                            || c.level as u8 > b.levels
                    })
                {
                    return Err(AuthoringError::Invalid("architecture source extent"));
                }
                SourceCommand::Architecture(commands.clone())
            }
            AuthoringIntent::SetRoof { pitch, style } => {
                if self.permission.unwrap_or(0) < 3 {
                    return Err(AuthoringError::Permission);
                }
                SourceCommand::SetRoof {
                    pitch: *pitch,
                    style: *style,
                }
            }
        };
        Ok(AuthoringWire::LotCommand {
            data: encode_client_command(self.actor.avatar_id, &command)?,
        })
    }
}
impl AuthoringState {
    pub fn invalidate_world_projection(&mut self) {
        self.world_projection_valid = false;
        self.diagonal_floor_tiles = None;
        if let Some(s) = self.snapshot.as_mut() {
            s.permission = None;
            s.budget = None;
            s.bounds = None;
            s.objects.clear();
            s.can_place_user = false;
            s.can_place_donated = false;
            if let Some(e) = s.eod.as_mut() {
                e.object_owner_id = None;
            }
        }
        if self.draft.as_ref().is_some_and(AuthoringIntent::is_world) {
            self.draft = None;
        }
        if self.pending.as_ref().is_some_and(|r| r.intent.is_world()) {
            self.status=OperationState::Unknown("The world changed before this edit was confirmed. Refresh the lot before choosing another edit.".into());
        }
    }
    /// Explicit recovery records uncertainty and never replays the old packet.
    pub fn abandon_pending(&mut self) {
        if let Some(request) = self.pending.take() {
            self.unknown_operations.push(request);
            self.draft = None;
            self.status=OperationState::Unknown("Stopped waiting. This edit may have reached the world; check its current state before trying again.".into());
        }
    }
    pub fn prepare(&self, intent: &AuthoringIntent) -> Result<AuthoringWire, AuthoringError> {
        if intent.is_world() && !self.world_projection_valid {
            return Err(AuthoringError::Missing("current world projection"));
        }
        self.snapshot
            .as_ref()
            .ok_or(AuthoringError::Missing("source authoring projection"))?
            .prepare(intent)
    }
    pub(crate) fn install_observation(
        &mut self,
        snapshot: AuthoringSnapshot,
    ) -> Result<(), AuthoringError> {
        let valid = self.world_projection_valid
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.actor == snapshot.actor);
        self.install(snapshot)?;
        self.world_projection_valid = valid;
        Ok(())
    }
    pub fn install(&mut self, snapshot: AuthoringSnapshot) -> Result<(), AuthoringError> {
        snapshot.validate()?;
        // The fence belongs to one authenticated native lot context. An EOD may
        // arrive before its first StateSync, after the old snapshot was withdrawn.
        if self
            .snapshot
            .as_ref()
            .is_none_or(|old| old.actor != snapshot.actor)
        {
            self.presentation_generation = None;
            self.diagonal_floor_tiles = None;
        }
        if let Some(old) = &self.snapshot {
            if old.actor == snapshot.actor && snapshot.revision < old.revision {
                return Err(AuthoringError::Stale);
            }
            if old.actor != snapshot.actor {
                if self.pending.is_some() {
                    self.abandon_pending();
                    self.status = OperationState::Unknown(
                        "The selected lot session changed before the action was confirmed.".into(),
                    );
                }
                self.draft = None;
            }
        }
        self.snapshot = Some(snapshot);
        self.world_projection_valid = true;
        Ok(())
    }
    pub fn submit(&mut self, intent: AuthoringIntent) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending.is_some() && !matches!(intent, AuthoringIntent::CloseEod) {
            return Err(AuthoringError::Pending);
        }
        let wire = self.prepare(&intent)?;
        if self.pending.is_some() {
            self.abandon_pending();
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or(AuthoringError::Missing("source authoring projection"))?;
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or(AuthoringError::Invalid("operation identity"))?;
        let request = AuthoringRequest {
            operation_id: self.next_operation,
            actor: snapshot.actor.clone(),
            revision: snapshot.revision,
            intent: intent.clone(),
            wire,
        };
        self.draft = Some(intent);
        self.status = OperationState::Pending;
        self.pending = Some(request.clone());
        Ok(request)
    }
    pub fn receive(&mut self, receipt: AuthoringReceipt) -> Result<(), AuthoringError> {
        let pending = self.pending.as_ref().ok_or(AuthoringError::Stale)?;
        let (id, actor) = match &receipt {
            AuthoringReceipt::Unknown {
                operation_id,
                actor,
                ..
            }
            | AuthoringReceipt::Rejected {
                operation_id,
                actor,
                ..
            }
            | AuthoringReceipt::Accepted {
                operation_id,
                actor,
                ..
            } => (*operation_id, actor.clone()),
        };
        if pending.operation_id != id
            || pending.actor != actor
            || self.snapshot.as_ref().is_none_or(|s| s.actor != actor)
        {
            return Err(AuthoringError::Stale);
        }
        match receipt {
            AuthoringReceipt::Unknown { message, .. } => {
                self.status = OperationState::Unknown(message)
            }
            AuthoringReceipt::Rejected { message, .. } => {
                self.status = OperationState::Rejected(message);
                self.pending = None;
            }
            AuthoringReceipt::Accepted {
                snapshot, amount, ..
            } => {
                snapshot.validate()?;
                if snapshot.actor != actor
                    || snapshot.revision <= pending.revision
                    || self
                        .snapshot
                        .as_ref()
                        .is_some_and(|current| snapshot.revision < current.revision)
                {
                    return Err(AuthoringError::Stale);
                }
                self.snapshot = Some(*snapshot);
                self.status = OperationState::Accepted { amount };
                self.pending = None;
                self.draft = None;
            }
        }
        Ok(())
    }
}
fn roomie_category(category: i8) -> bool {
    (12..=20).contains(&category)
}
fn builder_category(category: i8) -> bool {
    roomie_category(category) || [0, 1, 2, 3, 4, 5, 7, 8, 9].contains(&category)
}
/// VMDefaultValidator / VMFSOCommunityValidator, without guessing a rank.
pub fn source_purchase_mode(
    permission: u8,
    community: bool,
    category: i8,
    desired: u8,
    inventory: bool,
) -> Result<u8, AuthoringError> {
    if !(1..=4).contains(&permission) || !(1..=2).contains(&desired) {
        return Err(AuthoringError::Permission);
    }
    if !inventory
        && permission != 4
        && if permission == 1 {
            !roomie_category(category)
        } else {
            !builder_category(category)
        }
    {
        return Err(AuthoringError::Permission);
    }
    Ok(if community {
        if desired == 1 && permission < 3 {
            2
        } else {
            desired
        }
    } else {
        1
    })
}
pub fn source_delete_mode(
    actor: u32,
    permission: u8,
    community: bool,
    object: &SourceObject,
    category: Option<i8>,
    desired: u8,
) -> Result<u8, AuthoringError> {
    if !(1..=2).contains(&desired) || permission > 4 {
        return Err(AuthoringError::Permission);
    }
    if desired == 1
        && (object.persist_id == 0
            || object.is_avatar
            || (community && (object.owner_id != actor || object.donated)))
    {
        return Err(AuthoringError::Permission);
    }
    if object.is_avatar && permission < 4 {
        return Err(AuthoringError::Permission);
    }
    if community {
        if permission >= 3 {
            return Ok(desired);
        }
        if permission < 2
            || !object.donated
            || !category.is_some_and(|c| builder_category(c) && !roomie_category(c))
        {
            return Err(AuthoringError::Permission);
        }
        Ok(desired)
    } else {
        if permission < 1 {
            return Err(AuthoringError::Permission);
        }
        Ok(
            if desired == 2 && object.persist_id != 0 && object.owner_id != actor && permission != 4
            {
                1
            } else {
                desired
            },
        )
    }
}
