//! Build previews are pure projections. A prepared edit becomes visible only after
//! an authenticated server adapter supplies a matching durable confirmation.
//! This module does not debit, refund, create database rows, or authenticate transport.
use super::{
    footprints::WorldObject,
    placement::{
        validate_floor, validate_placement, IntersectionCall, IntersectionScripts,
        PlaceRequestFlags, PlacementError, PlacementRequest,
    },
    rooms::{Portal, PortalId},
    tiles::{Facing, LotPosition, TilePos, WallTile},
    WorldError, WorldState,
};
use crate::ids::{EntityRef, ObjectId, PersistentId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const BUILD_SCHEMA_VERSION: u16 = 1;
pub const MAX_BUILD_EDITS: usize = 1_024;
pub const MAX_BUILD_OUTCOMES: usize = 4_096;
pub const MAX_UNDO_RECORDS: usize = 64;
pub const MAX_BUILD_SCRIPT_DECISIONS: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildEdit {
    SetWall {
        tile: TilePos,
        wall: WallTile,
    },
    SetFloor {
        tile: TilePos,
        pattern: u16,
    },
    SetTerrainVertex {
        x: u16,
        y: u16,
        height: i16,
    },
    PlaceObject {
        catalog_id: u32,
        object: WorldObject,
    },
    MoveObject {
        entity: EntityRef,
        expected_revision: u64,
        position: LotPosition,
        facing: Facing,
    },
    DeleteObject {
        entity: EntityRef,
        expected_revision: u64,
    },
    SetPortal {
        portal: Portal,
    },
    RemovePortal {
        id: PortalId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildIntent {
    pub operation: u64,
    pub actor: EntityRef,
    pub expected_architecture: u64,
    pub edits: Vec<BuildEdit>,
}

impl BuildIntent {
    pub fn new(
        operation: u64,
        actor: EntityRef,
        expected_architecture: u64,
        edits: Vec<BuildEdit>,
    ) -> Self {
        Self {
            operation,
            actor,
            expected_architecture,
            edits,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildPriceTable {
    pub wall_side: i64,
    pub diagonal_wall: i64,
    pub floor: i64,
    pub terrain_height_step: i64,
    pub objects: BTreeMap<u32, i64>,
}

impl BuildPriceTable {
    fn valid(&self) -> bool {
        self.objects.len() <= 65_536
            && [
                self.wall_side,
                self.diagonal_wall,
                self.floor,
                self.terrain_height_step,
            ]
            .iter()
            .chain(self.objects.values())
            .all(|p| *p >= 0 && *p <= 1_000_000_000_000)
    }
    fn quote(&self, edit: &BuildEdit, world: &WorldState) -> Result<i64, BuildError> {
        Ok(match edit {
            BuildEdit::PlaceObject { catalog_id, .. } => *self
                .objects
                .get(catalog_id)
                .ok_or(BuildError::UnknownCatalogItem)?,
            BuildEdit::SetWall { tile, wall } => {
                let old = &world
                    .lot
                    .tile(*tile)
                    .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?
                    .wall;
                i64::from((wall.sides & !old.sides).count_ones()) * self.wall_side
                    + if wall.diagonal != super::tiles::Diagonal::None
                        && wall.diagonal != old.diagonal
                    {
                        self.diagonal_wall
                    } else {
                        0
                    }
            }
            BuildEdit::SetFloor { tile, pattern } => {
                if *pattern != 0
                    && world
                        .lot
                        .tile(*tile)
                        .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?
                        .floor
                        != *pattern
                {
                    self.floor
                } else {
                    0
                }
            }
            BuildEdit::SetTerrainVertex { x, y, height } => {
                let old = world
                    .lot
                    .terrain_vertex(*x, *y)
                    .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?;
                (i64::from(*height) - i64::from(old)).abs() * self.terrain_height_step
            }
            _ => 0,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndoPolicy {
    #[default]
    Disabled,
    Compensating {
        version: u16,
        max_records: u16,
    },
}

/// Supplied from authoritative permissions/account/catalog projections, never a client quote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildAuthority {
    pub actor: EntityRef,
    pub owner: PersistentId,
    pub can_build: bool,
    pub can_manage_others: bool,
    pub connected: bool,
    pub permissions_revision: u64,
    pub account_revision: u64,
    pub catalog_revision: u64,
    pub balance: i64,
    pub prices: BuildPriceTable,
    pub undo_policy: UndoPolicy,
}

impl BuildAuthority {
    pub fn new(actor: EntityRef, owner: PersistentId, balance: i64) -> Self {
        Self {
            actor,
            owner,
            can_build: false,
            can_manage_others: false,
            connected: true,
            permissions_revision: 0,
            account_revision: 0,
            catalog_revision: 0,
            balance,
            prices: BuildPriceTable::default(),
            undo_policy: UndoPolicy::Disabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildError {
    InvalidRequest,
    NotAuthorized,
    Disconnected,
    StalePreview,
    StaleObject,
    UnknownCatalogItem,
    InsufficientFunds,
    Busy,
    OperationConflict,
    Placement(PlacementError),
    ScriptProtocol,
    World(WorldError),
    QuoteOverflow,
    InvalidConfirmation,
    HistoryFull,
    NoUndo,
    UndoVersion,
    InvalidState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildChangeSet {
    pub tiles: BTreeSet<TilePos>,
    pub entities: BTreeSet<EntityRef>,
    pub allocated_ids: BTreeSet<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ScriptDecision {
    call: IntersectionCall,
    allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildPreview {
    pub version: u16,
    pub intent: BuildIntent,
    pub owner: PersistentId,
    pub cost: i64,
    pub permissions_revision: u64,
    pub account_revision: u64,
    pub catalog_revision: u64,
    pub changes: BuildChangeSet,
    pub preview_hash: [u8; 32],
    expected_objects: BTreeMap<ObjectId, Option<(EntityRef, u64)>>,
    script_decisions: Vec<ScriptDecision>,
    inverse: Option<Vec<BuildEdit>>,
    undo_policy: UndoPolicy,
    undo_of: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurablePurchase {
    pub entity: EntityRef,
    pub catalog_id: u32,
    pub owner: PersistentId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableBuildRequest {
    pub operation: u64,
    pub actor: EntityRef,
    pub owner: PersistentId,
    pub preview_hash: [u8; 32],
    pub cost: i64,
    pub permissions_revision: u64,
    pub account_revision: u64,
    pub catalog_revision: u64,
    pub purchases: Vec<DurablePurchase>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurableBuildStatus {
    Committed,
    Rejected { reason: BuildError },
}

/// Admission of this type is server-only. A receipt number by itself is not proof;
/// the runtime must accept it only from its authenticated durable-effect completion lane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBuildConfirmation {
    pub operation: u64,
    pub actor: EntityRef,
    pub owner: PersistentId,
    pub preview_hash: [u8; 32],
    pub charged_cost: i64,
    pub durable_receipt: u64,
    pub status: DurableBuildStatus,
    pub created_objects: BTreeMap<EntityRef, PersistentId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildCommitStatus {
    Committed {
        architecture_revision: u64,
        object_revision: u64,
        created_objects: BTreeMap<EntityRef, PersistentId>,
    },
    Rejected {
        reason: BuildError,
    },
    /// Durable cost is already committed but an out-of-band trusted edit invalidated geometry.
    /// E must reconcile/refund the receipt; the mirror neither spends nor silently discards it.
    NeedsReconciliation {
        durable_receipt: u64,
        reason: BuildError,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCommitResult {
    pub operation: u64,
    pub preview_hash: [u8; 32],
    pub cost: i64,
    pub status: BuildCommitStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildBegin {
    Effect(DurableBuildRequest),
    AlreadyCompleted(BuildCommitResult),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingBuild {
    preview: BuildPreview,
    request: DurableBuildRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct UndoRecord {
    operation: u64,
    owner: PersistentId,
    version: u16,
    architecture_revision: u64,
    expected_objects: BTreeMap<ObjectId, Option<(EntityRef, u64)>>,
    inverse: Vec<BuildEdit>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildState {
    pending: Option<PendingBuild>,
    outcomes: BTreeMap<u64, BuildCommitResult>,
    confirmations: BTreeMap<u64, ServerBuildConfirmation>,
    undo: Vec<UndoRecord>,
}

impl BuildState {
    pub fn pending_operation(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.request.operation)
    }
    pub fn pending_effect(&self) -> Option<&DurableBuildRequest> {
        self.pending.as_ref().map(|p| &p.request)
    }
    pub(crate) fn pending_preview(&self) -> Option<&BuildPreview> {
        self.pending.as_ref().map(|p| &p.preview)
    }
    /// A source initialization failure must retain the already durable receipt.
    /// The integration rolls back projected geometry before recording this result.
    pub(crate) fn reconcile_runtime_failure(
        &mut self,
        confirmation: ServerBuildConfirmation,
        reason: BuildError,
    ) -> BuildCommitResult {
        let result = BuildCommitResult {
            operation: confirmation.operation,
            preview_hash: confirmation.preview_hash,
            cost: confirmation.charged_cost,
            status: BuildCommitStatus::NeedsReconciliation {
                durable_receipt: confirmation.durable_receipt,
                reason,
            },
        };
        self.pending = None;
        self.confirmations.insert(result.operation, confirmation);
        self.outcomes.insert(result.operation, result.clone());
        result
    }
    pub fn outcome(&self, operation: u64) -> Option<&BuildCommitResult> {
        self.outcomes.get(&operation)
    }
    pub fn outcomes(&self) -> &BTreeMap<u64, BuildCommitResult> {
        &self.outcomes
    }
    pub fn is_locked(
        &self,
        entity: EntityRef,
        old: Option<&WorldObject>,
        new: Option<&WorldObject>,
    ) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            let p = &pending.preview;
            (p.intent.actor == entity && new.is_none())
                || p.changes.entities.contains(&entity)
                || p.changes.allocated_ids.contains(&entity.object_id)
                || old.into_iter().chain(new).any(|o| {
                    o.rects()
                        .iter()
                        .flat_map(|r| r.tiles())
                        .any(|t| p.changes.tiles.contains(&t))
                })
        })
    }
    pub fn preview<S: IntersectionScripts + ?Sized>(
        world: &WorldState,
        intent: BuildIntent,
        authority: &BuildAuthority,
        scripts: &mut S,
    ) -> Result<BuildPreview, BuildError> {
        check_authority(&intent, authority)?;
        if intent.operation == 0
            || intent.edits.is_empty()
            || intent.edits.len() > MAX_BUILD_EDITS
            || !authority.prices.valid()
        {
            return Err(BuildError::InvalidRequest);
        }
        if world.object(intent.actor).is_none() {
            return Err(BuildError::StaleObject);
        }
        if intent.expected_architecture != world.lot.revision().architecture {
            return Err(BuildError::StalePreview);
        }
        if world.builds.pending_operation().is_some() {
            return Err(BuildError::Busy);
        }
        if world.builds.outcomes.contains_key(&intent.operation) {
            return Err(BuildError::OperationConflict);
        }
        let mut decisions = Vec::new();
        let mut script_limit = false;
        let mut recording = |call: &IntersectionCall| {
            if decisions.len() >= MAX_BUILD_SCRIPT_DECISIONS {
                script_limit = true;
                return false;
            }
            let allowed = scripts.allows_intersection(call);
            decisions.push(ScriptDecision {
                call: call.clone(),
                allowed,
            });
            allowed
        };
        let projection = project_edits(world, &intent, authority, &mut recording);
        if script_limit {
            return Err(BuildError::ScriptProtocol);
        }
        let projection = projection?;
        if projection.cost > authority.balance {
            return Err(BuildError::InsufficientFunds);
        }
        let mut expected = BTreeMap::new();
        for (id, object) in world.objects() {
            if projection.changes.entities.contains(&object.entity)
                || object
                    .rects()
                    .iter()
                    .flat_map(|r| r.tiles())
                    .any(|t| projection.changes.tiles.contains(&t))
            {
                expected.insert(*id, Some((object.entity, object.revision)));
            }
        }
        for id in &projection.changes.allocated_ids {
            expected.entry(*id).or_insert(None);
        }
        let mut preview = BuildPreview {
            version: BUILD_SCHEMA_VERSION,
            intent,
            owner: authority.owner,
            cost: projection.cost,
            permissions_revision: authority.permissions_revision,
            account_revision: authority.account_revision,
            catalog_revision: authority.catalog_revision,
            changes: projection.changes,
            preview_hash: [0; 32],
            expected_objects: expected,
            script_decisions: decisions,
            inverse: projection.inverse,
            undo_policy: authority.undo_policy,
            undo_of: None,
        };
        preview.preview_hash = hash_preview(&preview)?;
        Ok(preview)
    }
    pub fn begin_commit(
        &mut self,
        world: &WorldState,
        preview: BuildPreview,
        authority: &BuildAuthority,
    ) -> Result<BuildBegin, BuildError> {
        if preview.version != BUILD_SCHEMA_VERSION
            || hash_preview(&preview)? != preview.preview_hash
        {
            return Err(BuildError::InvalidRequest);
        }
        if let Some(done) = self.outcomes.get(&preview.intent.operation) {
            return if done.preview_hash == preview.preview_hash {
                Ok(BuildBegin::AlreadyCompleted(done.clone()))
            } else {
                Err(BuildError::OperationConflict)
            };
        }
        if let Some(pending) = &self.pending {
            return if pending.preview == preview {
                Ok(BuildBegin::Effect(pending.request.clone()))
            } else if pending.request.operation == preview.intent.operation {
                Err(BuildError::OperationConflict)
            } else {
                Err(BuildError::Busy)
            };
        }
        if self.outcomes.len() >= MAX_BUILD_OUTCOMES {
            return Err(BuildError::HistoryFull);
        }
        check_authority(&preview.intent, authority)?;
        if preview.owner != authority.owner
            || preview.permissions_revision != authority.permissions_revision
            || preview.account_revision != authority.account_revision
            || preview.catalog_revision != authority.catalog_revision
        {
            return Err(BuildError::StalePreview);
        }
        if preview.cost > authority.balance {
            return Err(BuildError::InsufficientFunds);
        }
        check_guards(&preview, world)?;
        let purchases = preview
            .intent
            .edits
            .iter()
            .filter_map(|edit| match edit {
                BuildEdit::PlaceObject { catalog_id, object } => Some(DurablePurchase {
                    entity: object.entity,
                    catalog_id: *catalog_id,
                    owner: preview.owner,
                }),
                _ => None,
            })
            .collect();
        let request = DurableBuildRequest {
            operation: preview.intent.operation,
            actor: preview.intent.actor,
            owner: preview.owner,
            preview_hash: preview.preview_hash,
            cost: preview.cost,
            permissions_revision: preview.permissions_revision,
            account_revision: preview.account_revision,
            catalog_revision: preview.catalog_revision,
            purchases,
        };
        self.pending = Some(PendingBuild {
            preview,
            request: request.clone(),
        });
        Ok(BuildBegin::Effect(request))
    }
    pub fn preview_undo<S: IntersectionScripts + ?Sized>(
        &self,
        world: &WorldState,
        operation: u64,
        new_operation: u64,
        authority: &BuildAuthority,
        scripts: &mut S,
    ) -> Result<BuildPreview, BuildError> {
        let record = self
            .undo
            .last()
            .filter(|r| r.operation == operation)
            .ok_or(BuildError::NoUndo)?;
        let version = match authority.undo_policy {
            UndoPolicy::Compensating { version, .. } => version,
            _ => return Err(BuildError::NoUndo),
        };
        if version != record.version || version != BUILD_SCHEMA_VERSION {
            return Err(BuildError::UndoVersion);
        }
        if record.owner != authority.owner && !authority.can_manage_others {
            return Err(BuildError::NotAuthorized);
        }
        if record.architecture_revision != world.lot.revision().architecture
            || !object_guards_match(&record.expected_objects, world)
        {
            return Err(BuildError::StalePreview);
        }
        let mut edits = record.inverse.clone();
        for edit in &mut edits {
            match edit {
                BuildEdit::MoveObject {
                    entity,
                    expected_revision,
                    ..
                }
                | BuildEdit::DeleteObject {
                    entity,
                    expected_revision,
                } => {
                    *expected_revision = world
                        .object(*entity)
                        .ok_or(BuildError::StaleObject)?
                        .revision
                }
                _ => {}
            }
        }
        let mut preview = Self::preview(
            world,
            BuildIntent::new(
                new_operation,
                authority.actor,
                world.lot.revision().architecture,
                edits,
            ),
            authority,
            scripts,
        )?;
        preview.undo_of = Some(operation);
        preview.preview_hash = hash_preview(&preview)?;
        Ok(preview)
    }
    pub fn validate(&self) -> Result<(), BuildError> {
        if self.outcomes.len() > MAX_BUILD_OUTCOMES
            || self.undo.len() > MAX_UNDO_RECORDS
            || self.outcomes.len() != self.confirmations.len()
            || self.outcomes.iter().any(|(id, result)| {
                *id == 0
                    || *id != result.operation
                    || self.confirmations.get(id).map_or(true, |c| {
                        c.operation != *id
                            || c.preview_hash != result.preview_hash
                            || !valid_terminal_pair(c, result)
                    })
            })
        {
            return Err(BuildError::InvalidState);
        }
        if let Some(p) = &self.pending {
            if p.preview.version != BUILD_SCHEMA_VERSION
                || p.preview.intent.operation == 0
                || p.preview.intent.edits.is_empty()
                || p.preview.intent.edits.len() > MAX_BUILD_EDITS
                || p.preview.expected_objects.len() > 32_767
                || p.preview.script_decisions.len() > MAX_BUILD_SCRIPT_DECISIONS
                || p.preview.preview_hash != hash_preview(&p.preview)?
                || p.preview.intent.operation != p.request.operation
                || p.preview.preview_hash != p.request.preview_hash
                || p.preview.owner.0 == 0
                || !p.preview.intent.actor.is_valid()
                || p.preview.cost < 0
                || p.preview.changes.entities.len() > 32_767
                || p.preview.changes.allocated_ids.len() > MAX_BUILD_EDITS
                || p.preview.changes.tiles.len() > super::lot::MAX_LOT_TILES
                || p.preview
                    .inverse
                    .as_ref()
                    .is_some_and(|inverse| inverse.is_empty() || inverse.len() > MAX_BUILD_EDITS)
                || p.request.actor != p.preview.intent.actor
                || p.request.owner != p.preview.owner
                || p.request.cost != p.preview.cost
                || p.request.permissions_revision != p.preview.permissions_revision
                || p.request.account_revision != p.preview.account_revision
                || p.request.catalog_revision != p.preview.catalog_revision
                || self.outcomes.contains_key(&p.request.operation)
            {
                return Err(BuildError::InvalidState);
            }
            let purchases: Vec<_> = p
                .preview
                .intent
                .edits
                .iter()
                .filter_map(|edit| match edit {
                    BuildEdit::PlaceObject { catalog_id, object } => Some(DurablePurchase {
                        entity: object.entity,
                        catalog_id: *catalog_id,
                        owner: p.preview.owner,
                    }),
                    _ => None,
                })
                .collect();
            if purchases != p.request.purchases
                || !valid_object_guards(&p.preview.expected_objects)
                || p.preview.changes.entities.iter().any(|r| !r.is_valid())
                || p.preview
                    .changes
                    .allocated_ids
                    .iter()
                    .any(|id| !id.is_valid())
                || p.preview.script_decisions.iter().any(|d| {
                    d.call.entrypoint != 5
                        || !d.call.caller.is_valid()
                        || !d.call.other.is_valid()
                        || !d.call.proposed_object.is_valid()
                        || !d.call.proposed_facing.valid()
                })
            {
                return Err(BuildError::InvalidState);
            }
        }
        let mut undo_operations = BTreeSet::new();
        if self.undo.iter().any(|u| {
            u.version != BUILD_SCHEMA_VERSION
                || u.inverse.len() > MAX_BUILD_EDITS
                || u.inverse.is_empty()
                || u.operation == 0
                || u.owner.0 == 0
                || !undo_operations.insert(u.operation)
                || !valid_object_guards(&u.expected_objects)
                || self.confirmations.get(&u.operation).map_or(true, |c| {
                    c.owner != u.owner || !matches!(c.status, DurableBuildStatus::Committed)
                })
                || !self
                    .outcomes
                    .get(&u.operation)
                    .is_some_and(|r| matches!(r.status, BuildCommitStatus::Committed { .. }))
        }) {
            return Err(BuildError::InvalidState);
        }
        Ok(())
    }
    pub fn validate_against(&self, world: &WorldState) -> Result<(), BuildError> {
        self.validate()?;
        if let Some(p) = &self.pending {
            if p.preview
                .changes
                .tiles
                .iter()
                .any(|tile| !world.lot.contains(*tile))
                || p.preview
                    .intent
                    .edits
                    .iter()
                    .any(|edit| !valid_edit_geometry(edit, world))
                || p.preview
                    .script_decisions
                    .iter()
                    .any(|d| !world.lot.contains_position(d.call.proposed_position))
                || p.preview.inverse.as_ref().is_some_and(|inverse| {
                    inverse.iter().any(|edit| !valid_edit_geometry(edit, world))
                })
            {
                return Err(BuildError::InvalidState);
            }
        }
        if self.undo.iter().any(|u| {
            u.inverse
                .iter()
                .any(|edit| !valid_edit_geometry(edit, world))
        }) {
            return Err(BuildError::InvalidState);
        }
        Ok(())
    }
}

fn valid_object_guards(guards: &BTreeMap<ObjectId, Option<(EntityRef, u64)>>) -> bool {
    guards.len() <= 32_767
        && guards.iter().all(|(id, expected)| {
            id.is_valid()
                && expected.map_or(true, |(entity, revision)| {
                    entity.object_id == *id && entity.is_valid() && revision > 0
                })
        })
}

fn valid_terminal_pair(confirmation: &ServerBuildConfirmation, result: &BuildCommitResult) -> bool {
    if !confirmation.actor.is_valid()
        || confirmation.owner.0 == 0
        || confirmation.charged_cost < 0
        || result.cost != confirmation.charged_cost
        || confirmation.created_objects.len() > MAX_BUILD_EDITS
        || confirmation
            .created_objects
            .iter()
            .any(|(entity, id)| !entity.is_valid() || id.0 == 0)
        || confirmation
            .created_objects
            .values()
            .collect::<BTreeSet<_>>()
            .len()
            != confirmation.created_objects.len()
    {
        return false;
    }
    match (&confirmation.status, &result.status) {
        (
            DurableBuildStatus::Committed,
            BuildCommitStatus::Committed {
                created_objects, ..
            },
        ) => confirmation.durable_receipt > 0 && created_objects == &confirmation.created_objects,
        (
            DurableBuildStatus::Committed,
            BuildCommitStatus::NeedsReconciliation {
                durable_receipt, ..
            },
        ) => confirmation.durable_receipt > 0 && *durable_receipt == confirmation.durable_receipt,
        (
            DurableBuildStatus::Rejected { reason },
            BuildCommitStatus::Rejected {
                reason: result_reason,
            },
        ) => {
            confirmation.charged_cost == 0
                && confirmation.created_objects.is_empty()
                && reason == result_reason
        }
        _ => false,
    }
}

fn valid_edit_geometry(edit: &BuildEdit, world: &WorldState) -> bool {
    match edit {
        BuildEdit::SetWall { tile, wall } => world.lot.contains(*tile) && wall.valid(),
        BuildEdit::SetFloor { tile, .. } => world.lot.contains(*tile),
        BuildEdit::SetTerrainVertex { x, y, .. } => {
            *x <= world.lot.width() && *y <= world.lot.height()
        }
        BuildEdit::PlaceObject { object, .. } => {
            object.valid() && world.lot.contains_position(object.position)
        }
        BuildEdit::MoveObject {
            entity,
            position,
            facing,
            ..
        } => entity.is_valid() && world.lot.contains_position(*position) && facing.valid(),
        BuildEdit::DeleteObject { entity, .. } => entity.is_valid(),
        BuildEdit::SetPortal { portal } => {
            portal.id.0 > 0
                && portal.entity.is_valid()
                && portal.cost > 0
                && portal.entry != portal.exit
                && world.lot.contains_position(portal.entry)
                && world.lot.contains_position(portal.exit)
        }
        BuildEdit::RemovePortal { id } => id.0 > 0,
    }
}

impl WorldState {
    pub fn preview_build<S: IntersectionScripts + ?Sized>(
        &self,
        intent: BuildIntent,
        authority: &BuildAuthority,
        scripts: &mut S,
    ) -> Result<BuildPreview, BuildError> {
        BuildState::preview(self, intent, authority, scripts)
    }
    pub fn begin_build_commit(
        &mut self,
        preview: BuildPreview,
        authority: &BuildAuthority,
    ) -> Result<BuildBegin, BuildError> {
        let mut builds = self.builds.clone();
        let result = builds.begin_commit(self, preview, authority)?;
        self.builds = builds;
        Ok(result)
    }
    pub fn complete_build_commit(
        &mut self,
        confirmation: ServerBuildConfirmation,
    ) -> Result<BuildCommitResult, BuildError> {
        if let Some(done) = self.builds.outcomes.get(&confirmation.operation) {
            return if self.builds.confirmations.get(&confirmation.operation) == Some(&confirmation)
            {
                Ok(done.clone())
            } else {
                Err(BuildError::InvalidConfirmation)
            };
        }
        let pending = self
            .builds
            .pending
            .clone()
            .ok_or(BuildError::InvalidConfirmation)?;
        let request = &pending.request;
        if confirmation.operation != request.operation
            || confirmation.actor != request.actor
            || confirmation.owner != request.owner
            || confirmation.preview_hash != request.preview_hash
        {
            return Err(BuildError::InvalidConfirmation);
        }
        let (status, cost) = match &confirmation.status {
            DurableBuildStatus::Rejected { reason } => {
                if confirmation.charged_cost != 0 || !confirmation.created_objects.is_empty() {
                    return Err(BuildError::InvalidConfirmation);
                }
                (
                    BuildCommitStatus::Rejected {
                        reason: reason.clone(),
                    },
                    0,
                )
            }
            DurableBuildStatus::Committed => {
                let purchase_refs: BTreeSet<_> =
                    request.purchases.iter().map(|p| p.entity).collect();
                let receipt_refs: BTreeSet<_> =
                    confirmation.created_objects.keys().copied().collect();
                let persistent_ids: BTreeSet<_> = confirmation
                    .created_objects
                    .values()
                    .map(|id| id.0)
                    .collect();
                if confirmation.durable_receipt == 0
                    || confirmation.charged_cost != request.cost
                    || purchase_refs != receipt_refs
                    || persistent_ids.contains(&0)
                    || persistent_ids.len() != confirmation.created_objects.len()
                {
                    return Err(BuildError::InvalidConfirmation);
                }
                match check_guards(&pending.preview, self) {
                    Err(reason) => (
                        BuildCommitStatus::NeedsReconciliation {
                            durable_receipt: confirmation.durable_receipt,
                            reason,
                        },
                        confirmation.charged_cost,
                    ),
                    Ok(()) => {
                        let mut decisions: VecDeque<_> =
                            pending.preview.script_decisions.iter().cloned().collect();
                        let mut protocol_error = false;
                        let mut replay = |call: &IntersectionCall| match decisions.pop_front() {
                            Some(d) if d.call == *call => d.allowed,
                            _ => {
                                protocol_error = true;
                                false
                            }
                        };
                        // Reuse the accepted quote; only geometry/scripts are reapplied. Prices and balances never mutate here.
                        let authority = BuildAuthority {
                            actor: request.actor,
                            owner: request.owner,
                            can_build: true,
                            can_manage_others: true,
                            connected: true,
                            permissions_revision: request.permissions_revision,
                            account_revision: request.account_revision,
                            catalog_revision: request.catalog_revision,
                            balance: i64::MAX,
                            prices: BuildPriceTable::default(),
                            undo_policy: pending.preview.undo_policy,
                        };
                        let projected =
                            apply_edits(self, &pending.preview.intent, &authority, &mut replay);
                        match projected {
                            Ok(mut projection) if !protocol_error && decisions.is_empty() => {
                                let arch = projection.world.lot.revision().architecture;
                                let objects = projection.world.object_revision();
                                let mut builds = self.builds.clone();
                                if let Some(operation) = pending.preview.undo_of {
                                    if builds.undo.last().map(|r| r.operation) == Some(operation) {
                                        builds.undo.pop();
                                    }
                                } else if let (
                                    Some(inverse),
                                    UndoPolicy::Compensating {
                                        version,
                                        max_records,
                                    },
                                ) =
                                    (&pending.preview.inverse, pending.preview.undo_policy)
                                {
                                    if version == BUILD_SCHEMA_VERSION && max_records > 0 {
                                        let bound = usize::from(max_records).min(MAX_UNDO_RECORDS);
                                        let expected = pending
                                            .preview
                                            .expected_objects
                                            .keys()
                                            .chain(pending.preview.changes.allocated_ids.iter())
                                            .copied()
                                            .map(|id| {
                                                (
                                                    id,
                                                    projection
                                                        .world
                                                        .objects()
                                                        .get(&id)
                                                        .map(|o| (o.entity, o.revision)),
                                                )
                                            })
                                            .collect();
                                        builds.undo.push(UndoRecord {
                                            operation: request.operation,
                                            owner: request.owner,
                                            version,
                                            architecture_revision: arch,
                                            expected_objects: expected,
                                            inverse: inverse.clone(),
                                        });
                                        if builds.undo.len() > bound {
                                            builds.undo.remove(0);
                                        }
                                    }
                                }
                                projection.world.builds = builds;
                                *self = projection.world;
                                (
                                    BuildCommitStatus::Committed {
                                        architecture_revision: arch,
                                        object_revision: objects,
                                        created_objects: confirmation.created_objects.clone(),
                                    },
                                    confirmation.charged_cost,
                                )
                            }
                            Ok(_) => (
                                BuildCommitStatus::NeedsReconciliation {
                                    durable_receipt: confirmation.durable_receipt,
                                    reason: BuildError::ScriptProtocol,
                                },
                                confirmation.charged_cost,
                            ),
                            Err(reason) => (
                                BuildCommitStatus::NeedsReconciliation {
                                    durable_receipt: confirmation.durable_receipt,
                                    reason,
                                },
                                confirmation.charged_cost,
                            ),
                        }
                    }
                }
            }
        };
        let result = BuildCommitResult {
            operation: confirmation.operation,
            preview_hash: confirmation.preview_hash,
            cost,
            status,
        };
        self.builds.pending = None;
        self.builds
            .confirmations
            .insert(result.operation, confirmation);
        self.builds
            .outcomes
            .insert(result.operation, result.clone());
        Ok(result)
    }
}

fn check_authority(intent: &BuildIntent, authority: &BuildAuthority) -> Result<(), BuildError> {
    if !authority.connected {
        return Err(BuildError::Disconnected);
    }
    if !authority.can_build
        || authority.actor != intent.actor
        || authority.actor.object_id.0 <= 0
        || authority.actor.generation == 0
        || authority.owner.0 == 0
    {
        return Err(BuildError::NotAuthorized);
    }
    Ok(())
}

fn object_guards_match(
    guards: &BTreeMap<ObjectId, Option<(EntityRef, u64)>>,
    world: &WorldState,
) -> bool {
    guards
        .iter()
        .all(|(id, expected)| world.objects().get(id).map(|o| (o.entity, o.revision)) == *expected)
}
fn check_guards(preview: &BuildPreview, world: &WorldState) -> Result<(), BuildError> {
    if preview.intent.expected_architecture != world.lot.revision().architecture {
        return Err(BuildError::StalePreview);
    }
    if world.object(preview.intent.actor).is_none()
        || !object_guards_match(&preview.expected_objects, world)
    {
        return Err(BuildError::StaleObject);
    }
    for (id, object) in world.objects() {
        if !preview.expected_objects.contains_key(id)
            && object
                .rects()
                .iter()
                .flat_map(|r| r.tiles())
                .any(|t| preview.changes.tiles.contains(&t))
        {
            return Err(BuildError::StaleObject);
        }
    }
    Ok(())
}
fn hash_preview(preview: &BuildPreview) -> Result<[u8; 32], BuildError> {
    let mut clone = preview.clone();
    clone.preview_hash = [0; 32];
    let bytes = bincode::serialize(&clone).map_err(|_| BuildError::InvalidRequest)?;
    Ok(Sha256::digest(bytes).into())
}

struct Projection {
    world: WorldState,
    cost: i64,
    changes: BuildChangeSet,
    inverse: Option<Vec<BuildEdit>>,
}
fn project_edits<S: IntersectionScripts + ?Sized>(
    world: &WorldState,
    intent: &BuildIntent,
    authority: &BuildAuthority,
    scripts: &mut S,
) -> Result<Projection, BuildError> {
    let mut pricing_world = world.clone();
    pricing_world.builds = BuildState::default();
    let mut cost = 0_i64;
    // Quotes are computed against each intermediate edit so repeated tiles do not charge twice.
    for edit in &intent.edits {
        cost = cost
            .checked_add(authority.prices.quote(edit, &pricing_world)?)
            .ok_or(BuildError::QuoteOverflow)?;
        apply_pricing_geometry(&mut pricing_world, edit)?;
    }
    let mut projection = apply_edits(world, intent, authority, scripts)?;
    projection.cost = cost;
    Ok(projection)
}

fn apply_pricing_geometry(world: &mut WorldState, edit: &BuildEdit) -> Result<(), BuildError> {
    match edit {
        BuildEdit::SetWall { tile, wall } => {
            world
                .lot
                .set_wall(*tile, wall.clone())
                .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
        }
        BuildEdit::SetFloor { tile, pattern } => {
            world
                .lot
                .set_floor(*tile, *pattern)
                .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
        }
        BuildEdit::SetTerrainVertex { x, y, height } => {
            world
                .lot
                .set_terrain_vertex(*x, *y, *height)
                .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
        }
        _ => {}
    }
    Ok(())
}

fn apply_edits<S: IntersectionScripts + ?Sized>(
    world: &WorldState,
    intent: &BuildIntent,
    authority: &BuildAuthority,
    scripts: &mut S,
) -> Result<Projection, BuildError> {
    let mut projected = world.clone();
    projected.builds = BuildState::default();
    let mut changes = BuildChangeSet::default();
    let mut inverse = Vec::new();
    let mut undoable = true;
    let mut object_edits = BTreeSet::new();
    for edit in &intent.edits {
        match edit {
            BuildEdit::SetWall { tile, wall } => {
                require_buildable(&projected, *tile)?;
                let old = projected
                    .lot
                    .tile(*tile)
                    .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?
                    .clone();
                if !wall.valid() {
                    return Err(BuildError::InvalidRequest);
                }
                if wall.sides != 0 && projected.lot.terrain_sloped(*tile) {
                    return Err(BuildError::Placement(PlacementError::CantPlaceOnSlope));
                }
                if tile.level > 1
                    && (wall.sides != 0 || wall.diagonal != super::tiles::Diagonal::None)
                    && !old.supported
                {
                    return Err(BuildError::Placement(PlacementError::Floor2NeedsSupport));
                }
                if old.wall.occupied & !wall.sides != 0 {
                    return Err(BuildError::Placement(
                        PlacementError::MustRemoveObjectsOnWall,
                    ));
                }
                if old.wall.occupied != wall.occupied {
                    return Err(BuildError::InvalidRequest);
                }
                inverse.push(BuildEdit::SetWall {
                    tile: *tile,
                    wall: old.wall,
                });
                projected
                    .lot
                    .set_wall(*tile, wall.clone())
                    .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
                changes.tiles.insert(*tile);
                for direction in super::tiles::Cardinal::ALL {
                    if let Some(p) = tile
                        .adjacent(direction)
                        .filter(|p| projected.lot.contains(*p))
                    {
                        changes.tiles.insert(p);
                    }
                }
                validate_objects_after_architecture(&projected, &changes.tiles)?;
            }
            BuildEdit::SetFloor { tile, pattern } => {
                require_buildable(&projected, *tile)?;
                let data = projected
                    .lot
                    .tile(*tile)
                    .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?;
                if *pattern != 0 && tile.level > 1 && !data.supported {
                    return Err(BuildError::Placement(PlacementError::CantPlaceInAir));
                }
                if *pattern >= super::tiles::FLOOR_WATER && tile.level > 1 {
                    let below = TilePos {
                        level: tile.level - 1,
                        ..*tile
                    };
                    if projected
                        .lot
                        .room_at(below.center())
                        .and_then(|r| projected.lot.rooms().room(r))
                        .map_or(true, |r| r.outside)
                    {
                        return Err(BuildError::Placement(PlacementError::CantPlaceInAir));
                    }
                }
                for object in projected
                    .objects()
                    .values()
                    .filter(|o| o.rects().iter().any(|r| r.tiles().contains(tile)))
                {
                    if let Some(error) = validate_floor(&object.rules, *pattern, tile.level) {
                        return Err(BuildError::Placement(error));
                    }
                }
                inverse.push(BuildEdit::SetFloor {
                    tile: *tile,
                    pattern: data.floor,
                });
                projected
                    .lot
                    .set_floor(*tile, *pattern)
                    .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
                changes.tiles.insert(*tile);
            }
            BuildEdit::SetTerrainVertex { x, y, height } => {
                let old = projected
                    .lot
                    .terrain_vertex(*x, *y)
                    .ok_or(BuildError::Placement(PlacementError::LocationOutOfBounds))?;
                let mut affected = BTreeSet::new();
                for dy in [-1_i16, 0] {
                    for dx in [-1_i16, 0] {
                        let p = TilePos::new(*x as i16 + dx, *y as i16 + dy, 1);
                        if projected.lot.contains(p) {
                            require_buildable(&projected, p)?;
                            affected.insert(p);
                        }
                    }
                }
                if affected.is_empty() {
                    return Err(BuildError::Placement(PlacementError::LocationOutOfBounds));
                }
                inverse.push(BuildEdit::SetTerrainVertex {
                    x: *x,
                    y: *y,
                    height: old,
                });
                projected
                    .lot
                    .set_terrain_vertex(*x, *y, *height)
                    .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
                for p in &affected {
                    if projected.lot.terrain_sloped(*p)
                        && projected.lot.tile(*p).is_some_and(|t| {
                            t.wall.sides != 0 || t.wall.diagonal != super::tiles::Diagonal::None
                        })
                    {
                        return Err(BuildError::Placement(PlacementError::CantPlaceOnSlope));
                    }
                }
                changes.tiles.extend(affected.iter().copied());
                validate_objects_after_architecture(&projected, &affected)?;
            }
            BuildEdit::PlaceObject { object, .. } => {
                if object.rules.is_avatar
                    || object.owner != Some(authority.owner)
                    || !object_edits.insert(object.entity.object_id)
                {
                    return Err(BuildError::NotAuthorized);
                }
                let mut request =
                    PlacementRequest::new(object.clone(), object.position, object.facing);
                request.flags = PlaceRequestFlags {
                    user_buildable_limit: true,
                    accept_slots: false,
                    ..PlaceRequestFlags::default()
                };
                let result = validate_placement(request, &projected, scripts)
                    .map_err(|_| BuildError::ScriptProtocol)?;
                if !result.is_success() {
                    return Err(BuildError::Placement(result.status));
                }
                changes
                    .tiles
                    .extend(object.rects().iter().flat_map(|r| r.tiles()));
                changes.entities.insert(object.entity);
                changes.allocated_ids.insert(object.entity.object_id);
                projected
                    .insert_object(object.clone())
                    .map_err(BuildError::World)?;
                inverse.push(BuildEdit::DeleteObject {
                    entity: object.entity,
                    expected_revision: projected.object(object.entity).expect("inserted").revision,
                });
            }
            BuildEdit::MoveObject {
                entity,
                expected_revision,
                position,
                facing,
            } => {
                if !object_edits.insert(entity.object_id) {
                    return Err(BuildError::InvalidRequest);
                }
                let object = projected
                    .object(*entity)
                    .ok_or(BuildError::StaleObject)?
                    .clone();
                check_object_authority(&object, *expected_revision, authority)?;
                if object.in_use {
                    return Err(BuildError::Placement(PlacementError::InUse));
                }
                let mut request = PlacementRequest::new(object.clone(), *position, *facing);
                request.flags.user_buildable_limit = true;
                let result = validate_placement(request, &projected, scripts)
                    .map_err(|_| BuildError::ScriptProtocol)?;
                if !result.is_success() {
                    return Err(BuildError::Placement(result.status));
                }
                changes
                    .tiles
                    .extend(object.rects().iter().flat_map(|r| r.tiles()));
                changes.entities.insert(*entity);
                projected
                    .move_object(*entity, *position, *facing)
                    .map_err(BuildError::World)?;
                let moved = projected.object(*entity).expect("moved");
                changes
                    .tiles
                    .extend(moved.rects().iter().flat_map(|r| r.tiles()));
                inverse.push(BuildEdit::MoveObject {
                    entity: *entity,
                    expected_revision: moved.revision,
                    position: object.position,
                    facing: object.facing,
                });
            }
            BuildEdit::DeleteObject {
                entity,
                expected_revision,
            } => {
                if !object_edits.insert(entity.object_id) {
                    return Err(BuildError::InvalidRequest);
                }
                let object = projected
                    .object(*entity)
                    .ok_or(BuildError::StaleObject)?
                    .clone();
                check_object_authority(&object, *expected_revision, authority)?;
                if object.in_use {
                    return Err(BuildError::Placement(PlacementError::CannotDeleteObject));
                }
                changes
                    .tiles
                    .extend(object.rects().iter().flat_map(|r| r.tiles()));
                changes.entities.insert(*entity);
                projected
                    .remove_object(*entity)
                    .map_err(BuildError::World)?;
                undoable = false;
            }
            BuildEdit::SetPortal { portal } => {
                let object = projected
                    .object(portal.entity)
                    .ok_or(BuildError::StaleObject)?;
                if !object.entrypoints.contains(&15)
                    || (object.owner != Some(authority.owner) && !authority.can_manage_others)
                {
                    return Err(BuildError::NotAuthorized);
                }
                require_buildable(
                    &projected,
                    portal.entry.tile().ok_or(BuildError::InvalidRequest)?,
                )?;
                require_buildable(
                    &projected,
                    portal.exit.tile().ok_or(BuildError::InvalidRequest)?,
                )?;
                if let Some(old) = projected.lot.portals().get(&portal.id) {
                    let previous_owner = projected
                        .object(old.entity)
                        .ok_or(BuildError::StaleObject)?;
                    if previous_owner.owner != Some(authority.owner) && !authority.can_manage_others
                    {
                        return Err(BuildError::NotAuthorized);
                    }
                    changes.entities.insert(old.entity);
                    changes.tiles.extend([
                        old.entry.tile().ok_or(BuildError::InvalidRequest)?,
                        old.exit.tile().ok_or(BuildError::InvalidRequest)?,
                    ]);
                    inverse.push(BuildEdit::SetPortal {
                        portal: old.clone(),
                    });
                } else {
                    inverse.push(BuildEdit::RemovePortal { id: portal.id });
                }
                changes.entities.insert(portal.entity);
                changes.tiles.extend([
                    portal.entry.tile().expect("checked"),
                    portal.exit.tile().expect("checked"),
                ]);
                projected
                    .lot
                    .upsert_portal(portal.clone())
                    .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
            }
            BuildEdit::RemovePortal { id } => {
                let portal = projected
                    .lot
                    .portals()
                    .get(id)
                    .ok_or(BuildError::StaleObject)?
                    .clone();
                let object = projected
                    .object(portal.entity)
                    .ok_or(BuildError::StaleObject)?;
                if object.owner != Some(authority.owner) && !authority.can_manage_others {
                    return Err(BuildError::NotAuthorized);
                }
                inverse.push(BuildEdit::SetPortal {
                    portal: portal.clone(),
                });
                changes.entities.insert(portal.entity);
                changes.tiles.extend([
                    portal.entry.tile().ok_or(BuildError::InvalidRequest)?,
                    portal.exit.tile().ok_or(BuildError::InvalidRequest)?,
                ]);
                projected
                    .lot
                    .remove_portal(*id)
                    .map_err(|e| BuildError::World(WorldError::Lot(e)))?;
            }
        }
    }
    inverse.reverse();
    Ok(Projection {
        world: projected,
        cost: 0,
        changes,
        inverse: if undoable { Some(inverse) } else { None },
    })
}
fn require_buildable(world: &WorldState, tile: TilePos) -> Result<(), BuildError> {
    if world.lot.buildable(tile) {
        Ok(())
    } else {
        Err(BuildError::Placement(PlacementError::LocationOutOfBounds))
    }
}
fn check_object_authority(
    object: &WorldObject,
    revision: u64,
    authority: &BuildAuthority,
) -> Result<(), BuildError> {
    if object.revision != revision {
        return Err(BuildError::StaleObject);
    }
    if object.rules.is_avatar
        || (!authority.can_manage_others && object.owner != Some(authority.owner))
    {
        return Err(BuildError::Placement(PlacementError::ObjectNotOwnedByYou));
    }
    Ok(())
}
fn validate_objects_after_architecture(
    world: &WorldState,
    tiles: &BTreeSet<TilePos>,
) -> Result<(), BuildError> {
    for object in world.objects().values().filter(|o| {
        o.rects()
            .iter()
            .flat_map(|r| r.tiles())
            .any(|p| tiles.contains(&p))
    }) {
        let request = PlacementRequest::new(object.clone(), object.position, object.facing);
        if let Some(error) = super::placement::validate_geometry(&request, world) {
            return Err(BuildError::Placement(error));
        }
    }
    Ok(())
}

#[cfg(test)]
mod restore_regressions {
    use super::*;

    fn committed() -> BuildState {
        let confirmation = ServerBuildConfirmation {
            operation: 1,
            actor: EntityRef {
                object_id: ObjectId(1),
                generation: 1,
            },
            owner: PersistentId(50),
            preview_hash: [1; 32],
            charged_cost: 10,
            durable_receipt: 99,
            status: DurableBuildStatus::Committed,
            created_objects: BTreeMap::new(),
        };
        let outcome = BuildCommitResult {
            operation: 1,
            preview_hash: [1; 32],
            cost: 10,
            status: BuildCommitStatus::Committed {
                architecture_revision: 1,
                object_revision: 1,
                created_objects: BTreeMap::new(),
            },
        };
        BuildState {
            pending: None,
            confirmations: BTreeMap::from([(1, confirmation)]),
            outcomes: BTreeMap::from([(1, outcome)]),
            undo: Vec::new(),
        }
    }

    #[test]
    fn restored_terminal_receipt_must_match_outcome_status_cost_identity_and_creation_map() {
        committed().validate().unwrap();
        for change in 0..7 {
            let mut state = committed();
            let c = state.confirmations.get_mut(&1).unwrap();
            let r = state.outcomes.get_mut(&1).unwrap();
            match change {
                0 => c.durable_receipt = 0,
                1 => c.owner = PersistentId(0),
                2 => {
                    c.actor = EntityRef {
                        object_id: ObjectId(0),
                        generation: 0,
                    }
                }
                3 => {
                    c.charged_cost = -100;
                    r.cost = -100;
                }
                4 => {
                    r.status = BuildCommitStatus::Rejected {
                        reason: BuildError::NotAuthorized,
                    }
                }
                5 => r.cost = 11,
                _ => {
                    c.created_objects.insert(
                        EntityRef {
                            object_id: ObjectId(2),
                            generation: 1,
                        },
                        PersistentId(100),
                    );
                }
            }
            let decoded: BuildState =
                bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
            assert_eq!(
                decoded.validate(),
                Err(BuildError::InvalidState),
                "case {change}"
            );
        }
    }
}
