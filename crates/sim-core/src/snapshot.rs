//! Versioned, bounded snapshots of the state after completed tick N.
//!
//! The accepted tail starts at N+1. Fixed-width little-endian bincode is checked
//! for canonical reserialization, then every owned semantic graph is validated
//! before a candidate state is returned. Callers atomically install that state.
//! SHA-256 detects corruption; source authentication and journal/epoch fencing
//! are responsibilities of the transport and persistence adapters.

use crate::avatars::{
    motives::MotiveDecay, social::RelationshipTarget as AvatarRelationshipTarget,
    timeline::AnimationMetadata, AvatarPlatform,
};
use crate::effects::{EffectKind, EffectPayload};
use crate::ids::{EntityRef, ObjectId};
use crate::state::{
    ContentDescriptor, ContentSet, ContinuationKind, EntityState, HeadlineKind, LifecyclePhase,
    SimState, SIMULATION_SCHEMA,
};
use crate::vm::{
    HostRequest, HostResponse, PrimitiveExit, RegisterTarget, RelationshipKey, RelationshipOwner,
    RelationshipTarget, VmMode, VmResolution, VmStop, VmThread,
};
use bincode::Options;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SNAPSHOT_MAGIC: [u8; 8] = *b"WLDSNAP\0";
pub const SNAPSHOT_FORMAT_VERSION: u16 = 1;
pub const SNAPSHOT_HEADER_LEN: usize = 108;
pub const SNAPSHOT_CHECKSUM_LEN: usize = 32;
pub const MAX_SNAPSHOT_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTITIES: u32 = 32_767;
const MAX_CONTINUATIONS: u32 = 32_767;
const MAX_DECODE_DEPTH: usize = 128;
const MAX_DECODE_COLLECTION: usize = 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotLimits {
    pub max_payload_bytes: u64,
    pub max_entities: u32,
    pub max_continuations: u32,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: MAX_SNAPSHOT_PAYLOAD_BYTES,
            max_entities: MAX_ENTITIES,
            max_continuations: MAX_CONTINUATIONS,
        }
    }
}

impl SnapshotLimits {
    pub fn validate(self) -> Result<(), SnapshotError> {
        if self.max_payload_bytes == 0 || self.max_payload_bytes > MAX_SNAPSHOT_PAYLOAD_BYTES {
            return Err(SnapshotError::InvalidLimits("payload byte limit"));
        }
        if self.max_entities == 0 || self.max_entities > MAX_ENTITIES {
            return Err(SnapshotError::InvalidLimits("entity count limit"));
        }
        if self.max_continuations == 0 || self.max_continuations > MAX_CONTINUATIONS {
            return Err(SnapshotError::InvalidLimits("continuation count limit"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotExpectation {
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub limits: SnapshotLimits,
}

impl SnapshotExpectation {
    pub fn new(lot_id: u64, authority_epoch: u64) -> Self {
        Self {
            lot_id,
            authority_epoch,
            limits: SnapshotLimits::default(),
        }
    }

    pub fn validate(self) -> Result<(), SnapshotError> {
        self.limits.validate()?;
        if self.lot_id == 0 || self.authority_epoch == 0 {
            return Err(SnapshotError::InvalidLimits(
                "expected lot and epoch must be nonzero",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    InvalidLimits(&'static str),
    TruncatedHeader,
    BadMagic,
    UnsupportedFormat(u16),
    UnsupportedSchema(u16),
    PayloadTooLarge {
        bytes: u64,
        limit: u64,
    },
    LengthMismatch {
        declared: u64,
        actual: u64,
    },
    ChecksumMismatch,
    LotMismatch {
        expected: u64,
        actual: u64,
    },
    EpochMismatch {
        expected: u64,
        actual: u64,
    },
    ContentMismatch,
    TuningMismatch,
    HeaderPayloadMismatch(&'static str),
    NonCanonicalPayload,
    TickOverflow,
    Encode(String),
    Decode(String),
    InvalidContent(String),
    InvalidState {
        component: &'static str,
        detail: String,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "simulation snapshot: {self:?}")
    }
}

impl std::error::Error for SnapshotError {}

fn invalid(component: &'static str, detail: impl Into<String>) -> SnapshotError {
    SnapshotError::InvalidState {
        component,
        detail: detail.into(),
    }
}

fn options(limit: u64) -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(limit)
        .reject_trailing_bytes()
}

fn canonical<T: Serialize>(value: &T, limit: u64) -> Result<Vec<u8>, SnapshotError> {
    options(limit)
        .serialize(value)
        .map_err(|e| SnapshotError::Encode(e.to_string()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Header {
    schema: u16,
    lot_id: u64,
    authority_epoch: u64,
    completed_tick: u64,
    content: ContentDescriptor,
    payload_len: u64,
}

impl Header {
    fn bytes(self) -> [u8; SNAPSHOT_HEADER_LEN] {
        let mut bytes = [0; SNAPSHOT_HEADER_LEN];
        bytes[..8].copy_from_slice(&SNAPSHOT_MAGIC);
        bytes[8..10].copy_from_slice(&SNAPSHOT_FORMAT_VERSION.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.schema.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.lot_id.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.authority_epoch.to_le_bytes());
        bytes[28..36].copy_from_slice(&self.completed_tick.to_le_bytes());
        bytes[36..68].copy_from_slice(&self.content.content_hash);
        bytes[68..100].copy_from_slice(&self.content.tuning_hash);
        bytes[100..108].copy_from_slice(&self.payload_len.to_le_bytes());
        bytes
    }

    fn read(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() < SNAPSHOT_HEADER_LEN + SNAPSHOT_CHECKSUM_LEN {
            return Err(SnapshotError::TruncatedHeader);
        }
        if bytes[..8] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::BadMagic);
        }
        let format = u16::from_le_bytes([bytes[8], bytes[9]]);
        if format != SNAPSHOT_FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedFormat(format));
        }
        let schema = u16::from_le_bytes([bytes[10], bytes[11]]);
        if schema != SIMULATION_SCHEMA {
            return Err(SnapshotError::UnsupportedSchema(schema));
        }
        let u64_at = |offset: usize| {
            let mut value = [0; 8];
            value.copy_from_slice(&bytes[offset..offset + 8]);
            u64::from_le_bytes(value)
        };
        let mut content_hash = [0; 32];
        content_hash.copy_from_slice(&bytes[36..68]);
        let mut tuning_hash = [0; 32];
        tuning_hash.copy_from_slice(&bytes[68..100]);
        Ok(Self {
            schema,
            lot_id: u64_at(12),
            authority_epoch: u64_at(20),
            completed_tick: u64_at(28),
            content: ContentDescriptor {
                content_hash,
                tuning_hash,
            },
            payload_len: u64_at(100),
        })
    }
}

/// Encodes a fully validated post-tick state. It never changes the source state.
pub fn encode(state: &SimState, content: &ContentSet) -> Result<Vec<u8>, SnapshotError> {
    validate_state(state, content)?;
    let payload = canonical(state, MAX_SNAPSHOT_PAYLOAD_BYTES)?;
    let header = Header {
        schema: state.schema,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        completed_tick: state.completed_tick,
        content: state.content,
        payload_len: payload.len() as u64,
    }
    .bytes();
    let mut bytes = Vec::with_capacity(SNAPSHOT_HEADER_LEN + payload.len() + SNAPSHOT_CHECKSUM_LEN);
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&payload);
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

/// Decodes into a new candidate; an error cannot partially replace live state.
///
/// Limits and the complete declared frame length are checked before payload
/// allocation. The guarded decoder also bounds nesting and collection hints.
pub fn decode(
    bytes: &[u8],
    content: &ContentSet,
    expectation: SnapshotExpectation,
) -> Result<SimState, SnapshotError> {
    expectation.validate()?;
    let header = Header::read(bytes)?;
    if header.payload_len > expectation.limits.max_payload_bytes {
        return Err(SnapshotError::PayloadTooLarge {
            bytes: header.payload_len,
            limit: expectation.limits.max_payload_bytes,
        });
    }
    let actual = (bytes.len() - SNAPSHOT_HEADER_LEN - SNAPSHOT_CHECKSUM_LEN) as u64;
    if header.payload_len != actual {
        return Err(SnapshotError::LengthMismatch {
            declared: header.payload_len,
            actual,
        });
    }
    if header.lot_id != expectation.lot_id {
        return Err(SnapshotError::LotMismatch {
            expected: expectation.lot_id,
            actual: header.lot_id,
        });
    }
    if header.authority_epoch != expectation.authority_epoch {
        return Err(SnapshotError::EpochMismatch {
            expected: expectation.authority_epoch,
            actual: header.authority_epoch,
        });
    }
    let end = bytes.len() - SNAPSHOT_CHECKSUM_LEN;
    let digest: [u8; 32] = Sha256::digest(&bytes[..end]).into();
    if bytes[end..] != digest {
        return Err(SnapshotError::ChecksumMismatch);
    }
    content.validate().map_err(SnapshotError::InvalidContent)?;
    let descriptor = content
        .descriptor()
        .map_err(SnapshotError::InvalidContent)?;
    check_descriptor(header.content, descriptor)?;
    let payload = &bytes[SNAPSHOT_HEADER_LEN..end];
    let state: SimState = options(expectation.limits.max_payload_bytes)
        .deserialize_seed(guard::ValueSeed::<SimState>::new(), payload)
        .map_err(|e| SnapshotError::Decode(e.to_string()))?;
    if canonical(&state, expectation.limits.max_payload_bytes)? != payload {
        return Err(SnapshotError::NonCanonicalPayload);
    }
    if state.schema != header.schema {
        return Err(SnapshotError::HeaderPayloadMismatch("schema"));
    }
    if state.lot_id != header.lot_id {
        return Err(SnapshotError::HeaderPayloadMismatch("lot ID"));
    }
    if state.authority_epoch != header.authority_epoch {
        return Err(SnapshotError::HeaderPayloadMismatch("authority epoch"));
    }
    if state.completed_tick != header.completed_tick {
        return Err(SnapshotError::HeaderPayloadMismatch("completed tick"));
    }
    if state.content != header.content {
        return Err(SnapshotError::HeaderPayloadMismatch(
            "content/tuning descriptor",
        ));
    }
    validate_counts(&state, expectation.limits)?;
    validate_state(&state, content)?;
    Ok(state)
}

fn check_descriptor(
    actual: ContentDescriptor,
    expected: ContentDescriptor,
) -> Result<(), SnapshotError> {
    if actual.content_hash != expected.content_hash {
        return Err(SnapshotError::ContentMismatch);
    }
    if actual.tuning_hash != expected.tuning_hash {
        return Err(SnapshotError::TuningMismatch);
    }
    Ok(())
}

fn validate_counts(state: &SimState, limits: SnapshotLimits) -> Result<(), SnapshotError> {
    if state.entities.len() > limits.max_entities as usize
        || state.entities.len() > state.limits.max_entities as usize
    {
        return Err(invalid(
            "entities",
            "entity count exceeds caller or stored simulation limit",
        ));
    }
    if state.continuations.len() > limits.max_continuations as usize
        || state.continuations.len() > state.limits.max_continuations as usize
    {
        return Err(invalid(
            "continuations",
            "continuation count exceeds caller or stored simulation limit",
        ));
    }
    Ok(())
}

/// Validates the complete simulation-local graph without mutating it.
/// Cached historical frame references remain generation-aware and may be stale;
/// live entity ownership, schedules and active operations may not be stale.
pub fn validate_state(state: &SimState, content: &ContentSet) -> Result<(), SnapshotError> {
    if state.schema != SIMULATION_SCHEMA {
        return Err(SnapshotError::UnsupportedSchema(state.schema));
    }
    if state.lot_id == 0 || state.authority_epoch == 0 {
        return Err(invalid("identity", "lot ID and epoch must be nonzero"));
    }
    state
        .completed_tick
        .checked_add(1)
        .ok_or(SnapshotError::TickOverflow)?;
    state.limits.validate().map_err(|e| invalid("limits", e))?;
    validate_counts(state, SnapshotLimits::default())?;
    content.validate().map_err(SnapshotError::InvalidContent)?;
    check_descriptor(
        state.content,
        content
            .descriptor()
            .map_err(SnapshotError::InvalidContent)?,
    )?;
    if state.globals.len() != 38 {
        return Err(invalid(
            "globals",
            "expected exactly 38 source global slots",
        ));
    }
    state
        .ts1_inventory
        .validate()
        .map_err(|e| invalid("ts1_inventory", e.to_string()))?;
    if state.mode == VmMode::Tso
        && (state.ts1_family_budget.is_some() || !state.ts1_inventory.inventories.is_empty())
    {
        return Err(invalid(
            "ts1_projection",
            "TSO state contains TS1 neighborhood projections",
        ));
    }
    if (state.completed_tick == 0) != state.last_tick_digest.is_none() {
        return Err(invalid(
            "tick",
            "accepted-tick digest does not match completed boundary",
        ));
    }
    state
        .clock
        .validate()
        .map_err(|e| invalid("clock", format!("{e:?}")))?;
    state
        .clock
        .utc_dotnet_ticks()
        .map_err(|e| invalid("clock", format!("{e:?}")))?;
    if state.clock.ticks != state.completed_tick {
        return Err(invalid("clock", "clock does not match completed tick"));
    }
    state
        .scheduler
        .validate(state.completed_tick)
        .map_err(|e| invalid("scheduler", format!("{e:?}")))?;
    state
        .ids
        .validate_state()
        .map_err(|e| invalid("allocator", e.to_string()))?;
    state
        .world
        .validate()
        .map_err(|e| invalid("world", format!("{e:?}")))?;
    state
        .effects
        .validate_context(state.completed_tick, state.authority_epoch)
        .map_err(|e| invalid("effects", e.to_string()))?;
    if state.ids.len() != state.entities.len()
        || state.threads.len() != state.entities.len()
        || state.world.objects().len() != state.entities.len()
    {
        return Err(invalid(
            "entities",
            "allocator, entity, thread and world counts differ",
        ));
    }
    if state.next_continuation == 0 {
        return Err(invalid("continuations", "next unused ID is zero"));
    }
    let relationship_counts = validate_relationships(state)?;
    let mut animations: BTreeMap<&str, Vec<&AnimationMetadata>> = BTreeMap::new();
    for metadata in content.animation_metadata() {
        animations
            .entry(&metadata.resource)
            .or_default()
            .push(metadata);
    }
    let mut persistent_groups = BTreeMap::new();
    for (id, entity) in &state.entities {
        validate_entity(
            state,
            content,
            &animations,
            &relationship_counts,
            *id,
            entity,
        )?;
        if entity.info.persistent_id != 0 {
            if let Some(previous) =
                persistent_groups.insert(entity.info.persistent_id, entity.info.base_object)
            {
                if previous != entity.info.base_object {
                    return Err(invalid(
                        "entities",
                        "persistent ID belongs to different object groups",
                    ));
                }
            }
        }
    }
    for entity in state.ids.live_refs() {
        live(state, entity)?;
    }
    for entity in state.scheduler.scheduled_entities() {
        live(state, entity)?;
    }
    validate_containment(state)?;
    let mut vm_continuation_ids = BTreeSet::new();
    for (id, thread) in &state.threads {
        if *id != thread.owner.object_id || thread.mode != state.mode {
            return Err(invalid("threads", "owner key or VM mode mismatch"));
        }
        live(state, thread.owner)?;
        thread
            .validate(content.routines())
            .map_err(|e| invalid("threads", e.to_string()))?;
        if let Some(continuation) = &thread.continuation {
            if continuation.request_id == 0
                || continuation.request_id >= state.next_continuation
                || !vm_continuation_ids.insert(continuation.request_id)
            {
                return Err(invalid(
                    "threads",
                    "continuation ID is reused or exceeds persisted counter",
                ));
            }
            if let Some(resolution) = &continuation.resolution {
                validate_resolution(thread, &continuation.request, resolution)?;
            }
        }
    }
    validate_continuations(state)?;
    Ok(())
}

fn live(state: &SimState, reference: EntityRef) -> Result<&EntityState, SnapshotError> {
    if !state.ids.is_live(reference) {
        return Err(invalid(
            "references",
            "reference is not a live allocated generation",
        ));
    }
    state
        .entities
        .get(&reference.object_id)
        .filter(|entity| entity.info.reference == reference)
        .ok_or_else(|| {
            invalid(
                "references",
                "entity table does not contain the live generation",
            )
        })
}

fn validate_entity(
    state: &SimState,
    content: &ContentSet,
    animations: &BTreeMap<&str, Vec<&AnimationMetadata>>,
    relationship_counts: &RelationshipCounts,
    id: ObjectId,
    entity: &EntityState,
) -> Result<(), SnapshotError> {
    let info = &entity.info;
    if id != info.reference.object_id {
        return Err(invalid("entities", "entity map key differs from identity"));
    }
    live(state, info.reference)?;
    if info.dead || entity.lifecycle == LifecyclePhase::Exited {
        return Err(invalid(
            "entities",
            "completed state retains a deleted/exited entity",
        ));
    }
    let definition = content
        .object(info.guid)
        .ok_or_else(|| invalid("entities", "object definition is missing"))?;
    if entity.attributes.len() < definition.attributes.len()
        || entity.attributes.len() > 4096
        || entity.object_data.len() != 80
        || entity.dynamic_sprite_flags.len() != 128
        || entity.type_attributes.len() > 4096
        || entity.tuning_overrides.len() > 65_536
        || entity
            .active_advertisements
            .as_ref()
            .is_some_and(|values| values.len() > 196_608)
        || entity.list.len() > state.limits.max_list_items as usize
        || entity.slots.len()
            != if info.is_avatar {
                3
            } else {
                usize::from(definition.slot_count)
            }
        || entity.pending_entrypoints.len() > 3
        || entity
            .pending_entrypoints
            .iter()
            .any(|entry| ![0, 8, 1].contains(entry))
        || entity.queued_users.len() > state.limits.max_entities as usize
        || entity.lockout_started > state.completed_tick
    {
        return Err(invalid(
            "entities",
            "entity storage, entrypoint or timestamp bounds are invalid",
        ));
    }
    for user in &entity.queued_users {
        live(state, *user)?;
    }
    if let Some(headline) = &entity.headline {
        if !(-491_520..=491_505).contains(&headline.duration)
            || matches!(
                &headline.kind,
                HeadlineKind::Balloon { icon: Some(icon), .. }
                    if icon.object_id.0 <= 0 || icon.generation == 0
            )
        {
            return Err(invalid(
                "headlines",
                "duration or cached icon identity is invalid",
            ));
        }
        // Source animation counters wrap. A cached icon may name a deleted
        // generation; restoration must retain that identity without rebinding.
    }
    let projection = state
        .world
        .object(info.reference)
        .ok_or_else(|| invalid("world", "entity projection is missing"))?;
    if projection.position.x != i32::from(info.position.x)
        || projection.position.y != i32::from(info.position.y)
        || i16::from(projection.position.level) != i16::from(info.position.level)
        || projection.facing.0 != info.direction
        || projection.rules.is_avatar != info.is_avatar
        || info.is_avatar != entity.avatar.is_some()
    {
        return Err(invalid(
            "world",
            "entity position, facing or avatar flag differs from projection",
        ));
    }
    if info.group.is_empty()
        || info.group.len() > state.limits.max_entities as usize
        || !info.group.contains(&id)
        || !info.group.contains(&info.base_object)
    {
        return Err(invalid(
            "groups",
            "object group bounds or base membership are invalid",
        ));
    }
    let base = state
        .entities
        .get(&info.base_object)
        .ok_or_else(|| invalid("groups", "base object is missing"))?
        .info
        .reference;
    let base_info = &state.entities[&info.base_object].info;
    if base_info.base_object != info.base_object
        || base_info.group != info.group
        || base_info.multi_tile != info.multi_tile
    {
        return Err(invalid("groups", "object group membership is inconsistent"));
    }
    // Compare each member's stored vector with the base exactly once. Walking
    // every peer vector from every member would make validation cubic in group
    // size, despite the outer serialized-byte bound.
    if id == info.base_object {
        let mut group = BTreeSet::new();
        for member in &info.group {
            let other = state
                .entities
                .get(member)
                .ok_or_else(|| invalid("groups", "object group member is missing"))?;
            if !group.insert(*member) || other.info.base_object != info.base_object {
                return Err(invalid("groups", "object group membership is inconsistent"));
            }
        }
    }
    if (!info.multi_tile && (info.group.len() != 1 || info.base_object != id))
        || projection.multitile_group != info.multi_tile.then_some(base)
    {
        return Err(invalid(
            "groups",
            "multitile projection differs from entity group",
        ));
    }
    if let Some(avatar) = &entity.avatar {
        avatar
            .validate()
            .map_err(|e| invalid("avatars", format!("{e:?}")))?;
        let platform = match state.mode {
            VmMode::Tso => AvatarPlatform::Tso,
            VmMode::Ts1 => AvatarPlatform::Ts1,
        };
        if avatar.entity != info.reference
            || avatar.persistent_id.0 != info.persistent_id
            || avatar.object_guid != info.guid
            || avatar.platform != platform
            || !avatar.has_thread
            || !entity.always_tick
            || avatar.motives.limits != content.tuning().motive_limits
        {
            return Err(invalid(
                "avatars",
                "avatar identity, mode, thread or motive tuning differs from owner",
            ));
        }
        if let MotiveDecay::Tso(decay) = &avatar.decay {
            if decay.tuning != content.tuning().tso_motives {
                return Err(invalid(
                    "avatars",
                    "embedded motive decay tuning differs from content",
                ));
            }
        }
        if avatar.relationships.values.len()
            != relationship_counts
                .matrices
                .get(&info.reference)
                .copied()
                .unwrap_or(0)
            || avatar.relationships.changed_persistent.len()
                != relationship_counts
                    .changed
                    .get(&info.reference)
                    .copied()
                    .unwrap_or(0)
        {
            return Err(invalid(
                "relationships",
                "avatar relationship projection count differs from shared book",
            ));
        }
        for (target, values) in &avatar.relationships.values {
            let target = match target {
                AvatarRelationshipTarget::Local(entity) => RelationshipTarget::Local(*entity),
                AvatarRelationshipTarget::Persistent(id) => RelationshipTarget::Persistent(id.0),
                AvatarRelationshipTarget::Neighbor(id) => RelationshipTarget::Neighbor(*id),
            };
            let key = RelationshipKey {
                owner: RelationshipOwner::Entity(info.reference),
                target,
            };
            if state.relationships.matrices.get(&key) != Some(values) {
                return Err(invalid(
                    "relationships",
                    "avatar relationship matrix differs from shared book",
                ));
            }
        }
        if avatar
            .relationships
            .changed_persistent
            .iter()
            .any(|target| {
                !state
                    .relationships
                    .changed_persistent
                    .contains(&(info.reference, target.0))
            })
        {
            return Err(invalid(
                "relationships",
                "avatar persistent relationship dirtiness differs from shared book",
            ));
        }
        if avatar
            .head_seek
            .target
            .is_some_and(|target| target.object_id.0 != avatar.person_data.values[41])
        {
            return Err(invalid(
                "avatars",
                "head-seek generation cache differs from raw target ID",
            ));
        }
        for animation in avatar
            .animations
            .animations
            .iter()
            .chain(avatar.animations.carry.iter())
        {
            if !animations
                .get(animation.metadata.resource.as_str())
                .is_some_and(|known| {
                    known
                        .iter()
                        .any(|metadata| **metadata == animation.metadata)
                })
            {
                return Err(invalid(
                    "animations",
                    "embedded animation metadata differs from content",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct RelationshipCounts {
    matrices: BTreeMap<EntityRef, usize>,
    changed: BTreeMap<EntityRef, usize>,
}

fn validate_relationships(state: &SimState) -> Result<RelationshipCounts, SnapshotError> {
    let book = &state.relationships;
    if book.matrices.len() > 65_536
        || book.changed_persistent.len() > 65_536
        || book.local_reverse.len() > 65_536
    {
        return Err(invalid(
            "relationships",
            "relationship collection count exceeds bounds",
        ));
    }
    book.validate()
        .map_err(|e| invalid("relationships", e.to_string()))?;
    let mut counts = RelationshipCounts::default();
    for key in book.matrices.keys() {
        if let RelationshipOwner::Entity(owner) = key.owner {
            live(state, owner)?;
            *counts.matrices.entry(owner).or_default() += 1;
        }
        if let RelationshipTarget::Local(target) = key.target {
            live(state, target)?;
        }
    }
    for (owner, target) in &book.changed_persistent {
        live(state, *owner)?;
        if *target == 0 {
            return Err(invalid("relationships", "persistent dirty target is zero"));
        }
        *counts.changed.entry(*owner).or_default() += 1;
    }
    let mut reverse_edges = 0_usize;
    for (target, owners) in &book.local_reverse {
        live(state, *target)?;
        reverse_edges = reverse_edges
            .checked_add(owners.len())
            .ok_or_else(|| invalid("relationships", "reverse relationship count overflow"))?;
        if owners.len() > state.limits.max_entities as usize || reverse_edges > 65_536 {
            return Err(invalid(
                "relationships",
                "reverse relationship count exceeds bounds",
            ));
        }
        for owner in owners {
            live(state, *owner)?;
        }
        // The source can dirty local reverse bookkeeping before a later
        // operation fails its matrix-size check. Do not require a matrix here.
    }
    Ok(counts)
}

fn validate_containment(state: &SimState) -> Result<(), SnapshotError> {
    for entity in state.entities.values() {
        if let Some((container, slot)) = entity.container {
            let parent = live(state, container)?;
            if parent.slots.get(usize::from(slot)) != Some(&Some(entity.info.reference))
                || parent.info.position != entity.info.position
            {
                return Err(invalid(
                    "containment",
                    "container backlink, slot or position mismatch",
                ));
            }
            if !state
                .world
                .object(entity.info.reference)
                .ok_or_else(|| invalid("world", "contained projection missing"))?
                .rules
                .zero_extent
            {
                return Err(invalid(
                    "containment",
                    "contained object has a physical extent",
                ));
            }
        }
        for (slot, occupant) in entity.slots.iter().enumerate() {
            if let Some(occupant) = occupant {
                if live(state, *occupant)?.container != Some((entity.info.reference, slot as u16)) {
                    return Err(invalid(
                        "containment",
                        "slot occupant has a different container backlink",
                    ));
                }
            }
        }
    }
    let mut finished = BTreeSet::new();
    for id in state.entities.keys() {
        let mut path = BTreeSet::new();
        let mut next = Some(*id);
        while let Some(current) = next {
            if finished.contains(&current) {
                break;
            }
            if !path.insert(current) {
                return Err(invalid("containment", "containment cycle"));
            }
            next = state
                .entities
                .get(&current)
                .ok_or_else(|| invalid("containment", "parent is missing"))?
                .container
                .map(|(parent, _)| parent.object_id);
        }
        finished.extend(path);
    }
    Ok(())
}

fn validate_resolution(
    thread: &VmThread,
    request: &HostRequest,
    resolution: &VmResolution,
) -> Result<(), SnapshotError> {
    let frame = thread
        .frames
        .last()
        .ok_or_else(|| invalid("threads", "resolved continuation has no frame"))?;
    if !matches!(
        resolution.response,
        HostResponse::Complete(_) | HostResponse::NextTick | HostResponse::AnimationEvent(_)
    ) || resolution.writes.len() > 1024
        || (matches!(resolution.response, HostResponse::AnimationEvent(_))
            && !matches!(request, HostRequest::Animation(_)))
    {
        return Err(invalid("threads", "invalid stored VM resolution kind"));
    }
    if matches!(request, HostRequest::External(_))
        && !matches!(
            resolution.response,
            HostResponse::Complete(
                PrimitiveExit::GotoTrue
                    | PrimitiveExit::GotoFalse
                    | PrimitiveExit::GotoTrueNextTick
                    | PrimitiveExit::GotoFalseNextTick
            )
        )
    {
        return Err(invalid(
            "effects",
            "external resolution must select a true/false branch",
        ));
    }
    for write in &resolution.writes {
        let bound = match write.target {
            RegisterTarget::Temp => thread.temps.len(),
            RegisterTarget::TempXl => thread.temp_xl.len(),
            RegisterTarget::Local => frame.locals.len(),
            RegisterTarget::Parameter => frame.args.len(),
            RegisterTarget::StackObjectId => 1,
        };
        if usize::from(write.index) >= bound {
            return Err(invalid(
                "threads",
                "stored response register index is out of bounds",
            ));
        }
    }
    Ok(())
}

fn validate_continuations(state: &SimState) -> Result<(), SnapshotError> {
    let mut effects = BTreeSet::new();
    let mut owners = BTreeSet::new();
    for (id, continuation) in &state.continuations {
        if *id == 0
            || *id != continuation.id
            || *id >= state.next_continuation
            || !owners.insert(continuation.entity)
        {
            return Err(invalid(
                "continuations",
                "ID/counter/key mismatch or multiple active continuations for owner",
            ));
        }
        live(state, continuation.entity)?;
        let thread = state
            .threads
            .get(&continuation.entity.object_id)
            .ok_or_else(|| invalid("continuations", "owner thread missing"))?;
        let waiting = if continuation.resumes_vm {
            let waiting = thread
                .continuation
                .as_ref()
                .ok_or_else(|| invalid("continuations", "VM continuation missing"))?;
            if waiting.request_id != *id
                || waiting.resolution.is_some()
                || thread.stop != (VmStop::Waiting { request_id: *id })
            {
                return Err(invalid(
                    "continuations",
                    "runtime operation does not own the waiting VM continuation",
                ));
            }
            Some(waiting)
        } else {
            if !thread.frames.is_empty() || thread.continuation.is_some() {
                return Err(invalid(
                    "continuations",
                    "standalone route owner is executing a BHAV",
                ));
            }
            None
        };
        match &continuation.kind {
            ContinuationKind::Effect(operation) => {
                let request = state
                    .effects
                    .request(*operation)
                    .ok_or_else(|| invalid("effects", "runtime effect has no pending operation"))?;
                let external = match waiting.map(|waiting| &waiting.request) {
                    Some(HostRequest::External(external)) => external,
                    _ => {
                        return Err(invalid(
                            "effects",
                            "effect is not attached to an external VM request",
                        ))
                    }
                };
                let expected_payload = canonical(
                    external,
                    u64::from(state.effects.limits().max_request_bytes),
                )?;
                if !continuation.resumes_vm
                    || !effects.insert(*operation)
                    || request.target != continuation.entity
                    || request.response_kind != EffectKind::Bytes
                    || request.payload != EffectPayload::Bytes(expected_payload)
                {
                    return Err(invalid(
                        "effects",
                        "effect ownership, response type or request payload mismatch",
                    ));
                }
            }
            ContinuationKind::Route(route) => {
                route
                    .validate_against(&state.world)
                    .map_err(|e| invalid("routes", format!("{e:?}")))?;
                if route.id() != *id
                    || route.actor() != continuation.entity
                    || route.parent_route_id().is_some()
                {
                    return Err(invalid(
                        "routes",
                        "route ID, root ownership or actor mismatch",
                    ));
                }
                // The route validator distinguishes active targets from
                // historical references retained by failed/completed routes.
                if continuation.resumes_vm
                    && !matches!(
                        waiting.map(|waiting| &waiting.request),
                        Some(HostRequest::Route(_))
                    )
                {
                    return Err(invalid(
                        "routes",
                        "route does not resume a VM route request",
                    ));
                }
                if state
                    .scheduler
                    .scheduled_tick(continuation.entity)
                    .is_none()
                {
                    return Err(invalid(
                        "routes",
                        "active route owner has no future schedule",
                    ));
                }
            }
        }
    }
    for request in state.effects.pending_requests() {
        live(state, request.target)?;
        if !effects.contains(&request.operation_id) {
            return Err(invalid(
                "effects",
                "pending effect has no runtime continuation",
            ));
        }
    }
    for thread in state.threads.values() {
        if let Some(waiting) = &thread.continuation {
            if waiting.resolution.is_none() {
                let runtime = state
                    .continuations
                    .get(&waiting.request_id)
                    .ok_or_else(|| {
                        invalid("continuations", "waiting VM has no runtime operation")
                    })?;
                if runtime.entity != thread.owner || !runtime.resumes_vm {
                    return Err(invalid(
                        "continuations",
                        "waiting VM belongs to a different runtime owner",
                    ));
                }
            } else if state.continuations.contains_key(&waiting.request_id) {
                return Err(invalid(
                    "continuations",
                    "resolved VM still has a pending runtime operation",
                ));
            }
        }
    }
    Ok(())
}

// Guarding the Serde traversal bounds recursion and allocation hints before
// individual module validation runs. Bincode's byte limit alone cannot bound a
// recursive Vec<RouteContinuation> or a malicious enormous sequence hint.
mod guard {
    use super::{MAX_DECODE_COLLECTION, MAX_DECODE_DEPTH};
    use serde::de::{
        DeserializeSeed, EnumAccess, Error, MapAccess, SeqAccess, VariantAccess, Visitor,
    };
    use serde::{Deserialize, Deserializer};
    use std::fmt;
    use std::marker::PhantomData;

    pub(super) struct ValueSeed<T>(PhantomData<T>);
    impl<T> ValueSeed<T> {
        pub(super) fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for ValueSeed<T> {
        type Value = T;
        fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
            T::deserialize(Bounded {
                inner: deserializer,
                depth: 0,
            })
        }
    }

    struct Bounded<D> {
        inner: D,
        depth: usize,
    }
    struct Seed<S> {
        inner: S,
        depth: usize,
    }
    impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<S> {
        type Value = S::Value;
        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            if self.depth > MAX_DECODE_DEPTH {
                return Err(D::Error::custom("snapshot nesting limit exceeded"));
            }
            self.inner.deserialize(Bounded {
                inner: deserializer,
                depth: self.depth,
            })
        }
    }

    struct GuardedVisitor<V> {
        inner: V,
        depth: usize,
    }
    macro_rules! scalar_visits {
        ($($method:ident($ty:ty)),* $(,)?) => { $(
            fn $method<E: Error>(self, value: $ty) -> Result<Self::Value, E> { self.inner.$method(value) }
        )* };
    }
    impl<'de, V: Visitor<'de>> Visitor<'de> for GuardedVisitor<V> {
        type Value = V::Value;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.inner.expecting(f)
        }
        scalar_visits!(
            visit_bool(bool),
            visit_i8(i8),
            visit_i16(i16),
            visit_i32(i32),
            visit_i64(i64),
            visit_i128(i128),
            visit_u8(u8),
            visit_u16(u16),
            visit_u32(u32),
            visit_u64(u64),
            visit_u128(u128),
            visit_f32(f32),
            visit_f64(f64),
            visit_char(char)
        );
        fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot string limit exceeded"));
            }
            self.inner.visit_str(value)
        }
        fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot string limit exceeded"));
            }
            self.inner.visit_borrowed_str(value)
        }
        fn visit_string<E: Error>(self, value: String) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot string limit exceeded"));
            }
            self.inner.visit_string(value)
        }
        fn visit_bytes<E: Error>(self, value: &[u8]) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot byte collection limit exceeded"));
            }
            self.inner.visit_bytes(value)
        }
        fn visit_borrowed_bytes<E: Error>(self, value: &'de [u8]) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot byte collection limit exceeded"));
            }
            self.inner.visit_borrowed_bytes(value)
        }
        fn visit_byte_buf<E: Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
            if value.len() > MAX_DECODE_COLLECTION {
                return Err(E::custom("snapshot byte collection limit exceeded"));
            }
            self.inner.visit_byte_buf(value)
        }
        fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
            self.inner.visit_unit()
        }
        fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
            self.inner.visit_none()
        }
        fn visit_some<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
            if self.depth >= MAX_DECODE_DEPTH {
                return Err(D::Error::custom("snapshot nesting limit exceeded"));
            }
            self.inner.visit_some(Bounded {
                inner,
                depth: self.depth + 1,
            })
        }
        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            inner: D,
        ) -> Result<Self::Value, D::Error> {
            if self.depth >= MAX_DECODE_DEPTH {
                return Err(D::Error::custom("snapshot nesting limit exceeded"));
            }
            self.inner.visit_newtype_struct(Bounded {
                inner,
                depth: self.depth + 1,
            })
        }
        fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
            if inner
                .size_hint()
                .is_some_and(|len| len > MAX_DECODE_COLLECTION)
            {
                return Err(A::Error::custom("snapshot sequence limit exceeded"));
            }
            self.inner.visit_seq(Sequence {
                inner,
                depth: self.depth + 1,
            })
        }
        fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
            if inner
                .size_hint()
                .is_some_and(|len| len > MAX_DECODE_COLLECTION)
            {
                return Err(A::Error::custom("snapshot map limit exceeded"));
            }
            self.inner.visit_map(Map {
                inner,
                depth: self.depth + 1,
            })
        }
        fn visit_enum<A: EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
            self.inner.visit_enum(Enum {
                inner,
                depth: self.depth + 1,
            })
        }
    }

    struct Sequence<A> {
        inner: A,
        depth: usize,
    }
    impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Sequence<A> {
        type Error = A::Error;
        fn next_element_seed<S: DeserializeSeed<'de>>(
            &mut self,
            seed: S,
        ) -> Result<Option<S::Value>, A::Error> {
            self.inner.next_element_seed(Seed {
                inner: seed,
                depth: self.depth,
            })
        }
        fn size_hint(&self) -> Option<usize> {
            self.inner.size_hint()
        }
    }
    struct Map<A> {
        inner: A,
        depth: usize,
    }
    impl<'de, A: MapAccess<'de>> MapAccess<'de> for Map<A> {
        type Error = A::Error;
        fn next_key_seed<S: DeserializeSeed<'de>>(
            &mut self,
            seed: S,
        ) -> Result<Option<S::Value>, A::Error> {
            self.inner.next_key_seed(Seed {
                inner: seed,
                depth: self.depth,
            })
        }
        fn next_value_seed<S: DeserializeSeed<'de>>(
            &mut self,
            seed: S,
        ) -> Result<S::Value, A::Error> {
            self.inner.next_value_seed(Seed {
                inner: seed,
                depth: self.depth,
            })
        }
        fn size_hint(&self) -> Option<usize> {
            self.inner.size_hint()
        }
    }
    struct Enum<A> {
        inner: A,
        depth: usize,
    }
    impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for Enum<A> {
        type Error = A::Error;
        type Variant = Variant<A::Variant>;
        fn variant_seed<S: DeserializeSeed<'de>>(
            self,
            seed: S,
        ) -> Result<(S::Value, Self::Variant), A::Error> {
            let (value, inner) = self.inner.variant_seed(Seed {
                inner: seed,
                depth: self.depth,
            })?;
            Ok((
                value,
                Variant {
                    inner,
                    depth: self.depth,
                },
            ))
        }
    }
    struct Variant<A> {
        inner: A,
        depth: usize,
    }
    impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Variant<A> {
        type Error = A::Error;
        fn unit_variant(self) -> Result<(), A::Error> {
            self.inner.unit_variant()
        }
        fn newtype_variant_seed<S: DeserializeSeed<'de>>(
            self,
            seed: S,
        ) -> Result<S::Value, A::Error> {
            self.inner.newtype_variant_seed(Seed {
                inner: seed,
                depth: self.depth + 1,
            })
        }
        fn tuple_variant<V: Visitor<'de>>(
            self,
            len: usize,
            visitor: V,
        ) -> Result<V::Value, A::Error> {
            self.inner.tuple_variant(
                len,
                GuardedVisitor {
                    inner: visitor,
                    depth: self.depth + 1,
                },
            )
        }
        fn struct_variant<V: Visitor<'de>>(
            self,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, A::Error> {
            self.inner.struct_variant(
                fields,
                GuardedVisitor {
                    inner: visitor,
                    depth: self.depth + 1,
                },
            )
        }
    }

    macro_rules! delegate {
        ($($method:ident $(($($name:ident: $ty:ty),*))?),* $(,)?) => { $(
            fn $method<V: Visitor<'de>>(self, $($($name: $ty,)*)? visitor: V) -> Result<V::Value, Self::Error> {
                if self.depth > MAX_DECODE_DEPTH { return Err(D::Error::custom("snapshot nesting limit exceeded")); }
                self.inner.$method($($($name,)*)? GuardedVisitor { inner: visitor, depth: self.depth })
            }
        )* };
    }
    impl<'de, D: Deserializer<'de>> Deserializer<'de> for Bounded<D> {
        type Error = D::Error;
        delegate!(deserialize_any, deserialize_bool, deserialize_i8, deserialize_i16, deserialize_i32, deserialize_i64, deserialize_i128, deserialize_u8, deserialize_u16, deserialize_u32, deserialize_u64, deserialize_u128, deserialize_f32, deserialize_f64, deserialize_char, deserialize_str, deserialize_bytes, deserialize_option, deserialize_unit,
            deserialize_unit_struct(name: &'static str), deserialize_newtype_struct(name: &'static str), deserialize_seq, deserialize_tuple(len: usize), deserialize_tuple_struct(name: &'static str, len: usize), deserialize_map, deserialize_struct(name: &'static str, fields: &'static [&'static str]), deserialize_enum(name: &'static str, variants: &'static [&'static str]), deserialize_identifier, deserialize_ignored_any);
        fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            // Slice-backed bincode supplies borrowed UTF-8 here, so the guarded
            // visitor checks its length before String's visitor allocates it.
            self.deserialize_str(visitor)
        }
        fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            self.deserialize_bytes(visitor)
        }
        fn is_human_readable(&self) -> bool {
            self.inner.is_human_readable()
        }
    }
}
