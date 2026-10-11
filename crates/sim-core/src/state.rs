//! Serializable replicated state and immutable content input for the runtime.
//!
//! These are sim-core-local integration types. Swarm F owns adoption into the
//! shared protocol. No server credentials, private EOD state, or budget ledger
//! are part of this state.
use crate::{
    avatars::{motives::TsoMotiveTuning, timeline::AnimationMetadata, AvatarState},
    clock::SimClock,
    effects::{EffectBook, OperationId},
    ids::{EntityRef, IdAllocator, ObjectId},
    rng::SimRng,
    scheduler::Scheduler,
    vm::{EntityInfo, RoutineKey, RoutineStore, VmMode, VmThread},
    world::{Footprint, PlacementRules, RouteContinuation, WorldState},
};
use bincode::Options;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const SIMULATION_SCHEMA: u16 = 2;
const CONTENT_SCHEMA: u16 = 1;
pub const MAX_CONTENT_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectDefinition {
    pub guid: u32,
    pub attributes: Vec<i16>,
    pub object_data: Vec<i16>,
    pub definition: Vec<i16>,
    pub master_definition: Vec<i16>,
    pub entry_points: BTreeMap<u8, RoutineKey>,
    pub entry_conditions: BTreeMap<u8, RoutineKey>,
    pub entry_point_count: u16,
    pub slot_count: u16,
    pub animation_table_id: u16,
    pub body_string_id: u16,
    pub footprint: Footprint,
    pub placement_rules: PlacementRules,
    pub master_guid: Option<u32>,
    pub level_offset: i8,
    pub family: i16,
}
impl ObjectDefinition {
    pub fn new(guid: u32, attribute_count: u16) -> Self {
        Self {
            guid,
            attributes: vec![0; usize::from(attribute_count)],
            object_data: vec![0; 80],
            definition: Vec::new(),
            master_definition: Vec::new(),
            entry_points: BTreeMap::new(),
            entry_conditions: BTreeMap::new(),
            entry_point_count: 33,
            slot_count: 0,
            animation_table_id: 0,
            body_string_id: 0,
            footprint: Footprint::default(),
            placement_rules: PlacementRules::default(),
            master_guid: None,
            level_offset: 0,
            family: 0,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.guid == 0
            || self.attributes.len() > 4096
            || self.object_data.len() != 80
            || self.definition.len() > 4096
            || self.master_definition.len() > 4096
            || self.slot_count > 256
            || self.entry_point_count > 256
            || self
                .entry_points
                .keys()
                .chain(self.entry_conditions.keys())
                .any(|key| u16::from(*key) >= self.entry_point_count)
            || !self.footprint.valid()
            || !self.placement_rules.valid()
        {
            return Err("invalid object definition bounds or geometry".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnimationKey {
    pub owner: u32,
    pub scope: u32,
    pub id: u16,
}
/// A normalized SLOT type-3 record supplied by the content importer. Owner zero
/// addresses the global SLOT resource; other owners are object GUIDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingSlot {
    pub search: crate::world::slots::SlotSearch,
    pub facing: i8,
    pub snap_to_direction: bool,
    pub snap_target_slot: Option<u16>,
}
impl RoutingSlot {
    pub fn validate(&self) -> Result<(), String> {
        let s = &self.search;
        if s.min_proximity < 0
            || (s.max_proximity != 0 && s.max_proximity < s.min_proximity)
            || s.min_proximity > 1024
            || s.max_proximity > 1024
            || s.resolution <= 0
            || s.resolution > 256
            || !(-4096..=4096).contains(&s.offset_x)
            || !(-4096..=4096).contains(&s.offset_y)
            || !(-4096..=4096).contains(&s.offset_z)
            || !(-3..=7).contains(&self.facing)
            || self.snap_target_slot.is_some_and(|slot| slot >= 256)
        {
            return Err("invalid normalized routing slot".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyJobUniform {
    pub male_mesh: String,
    pub female_mesh: Option<String>,
    pub texture: String,
}
impl LegacyJobUniform {
    pub fn validate(&self) -> Result<(), String> {
        if self.male_mesh.len() > 16384
            || self.female_mesh.as_ref().is_some_and(|s| s.len() > 16384)
            || self.texture.len() > 16384
        {
            Err("job uniform string bounds".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TuningSet {
    pub values: BTreeMap<(u32, u16, u16), i16>,
    pub tso_motives: Option<TsoMotiveTuning>,
    pub motive_limits: [i16; 16],
    pub relationship_multipliers: BTreeMap<u8, f32>,
    /// TSO special/0/0 tuning equals 1f. TS1 ignores this feature gate.
    pub fire_enabled: bool,
}
impl Default for TuningSet {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            tso_motives: None,
            motive_limits: [100; 16],
            relationship_multipliers: BTreeMap::new(),
            fire_enabled: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentDescriptor {
    pub content_hash: [u8; 32],
    pub tuning_hash: [u8; 32],
}

/// Source-order local/global TTAB definitions. Absence differs from an empty local TTAB.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionTable {
    pub local_table_present: bool,
    pub definitions: Vec<crate::interactions::InteractionDefinition>,
}

/// Source TTAB scoring metadata. Float bits and source indices remain exact;
/// UI offers carry check-time overrides separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionAdvertisement {
    pub motives: Vec<crate::avatars::advertisements::MotiveAdvertisement>,
    pub attenuation_code: u32,
    pub attenuation_value_bits: u32,
    pub autonomy_threshold: u32,
    pub joining_index: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentSet {
    routines: RoutineStore,
    objects: BTreeMap<u32, ObjectDefinition>,
    animations: BTreeMap<AnimationKey, AnimationMetadata>,
    routing_slots: BTreeMap<(u32, u16), RoutingSlot>,
    strings: BTreeMap<(u32, u16), Vec<String>>,
    named_trees: BTreeMap<(u32, String), RoutineKey>,
    suits: BTreeMap<(u32, u8, u16), crate::vm::ResolvedSuit>,
    legacy_job_uniforms: BTreeMap<(i16, i16), LegacyJobUniform>,
    tuning: TuningSet,
    interaction_tables: BTreeMap<u32, InteractionTable>,
    build_catalog: BTreeMap<u32, u32>,
    interaction_advertisements:
        BTreeMap<(u32, crate::interactions::InteractionKey), InteractionAdvertisement>,
}
impl ContentSet {
    pub fn new(
        routines: RoutineStore,
        objects: Vec<ObjectDefinition>,
        animations: Vec<(AnimationKey, AnimationMetadata)>,
        tuning: TuningSet,
    ) -> Result<Self, String> {
        let mut object_map = BTreeMap::new();
        for object in objects {
            let guid = object.guid;
            if object_map.insert(guid, object).is_some() {
                return Err("duplicate object GUID".into());
            }
        }
        let mut animation_map = BTreeMap::new();
        for (key, metadata) in animations {
            if animation_map.insert(key, metadata).is_some() {
                return Err("duplicate animation identity".into());
            }
        }
        let result = Self {
            routines,
            objects: object_map,
            animations: animation_map,
            routing_slots: BTreeMap::new(),
            strings: BTreeMap::new(),
            named_trees: BTreeMap::new(),
            suits: BTreeMap::new(),
            legacy_job_uniforms: BTreeMap::new(),
            tuning,
            interaction_tables: BTreeMap::new(),
            build_catalog: BTreeMap::new(),
            interaction_advertisements: BTreeMap::new(),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn interaction_table(&self, guid: u32) -> Option<&InteractionTable> {
        self.interaction_tables.get(&guid)
    }
    /// Catalog identities are provided by source catalog content; they are not GUIDs.
    pub fn with_build_catalog(mut self, entries: Vec<(u32, u32)>) -> Result<Self, String> {
        for (catalog, guid) in entries {
            if self.build_catalog.insert(catalog, guid).is_some() {
                return Err("duplicate build catalog identity".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn catalog_guid(&self, catalog: u32) -> Option<u32> {
        self.build_catalog.get(&catalog).copied()
    }
    pub fn with_interaction_advertisements(
        mut self,
        entries: Vec<(
            (u32, crate::interactions::InteractionKey),
            InteractionAdvertisement,
        )>,
    ) -> Result<Self, String> {
        for (key, metadata) in entries {
            if self
                .interaction_advertisements
                .insert(key, metadata)
                .is_some()
            {
                return Err("duplicate interaction advertisement".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn interaction_advertisement(
        &self,
        guid: u32,
        key: crate::interactions::InteractionKey,
    ) -> Option<&InteractionAdvertisement> {
        self.interaction_advertisements.get(&(guid, key))
    }
    pub fn with_interaction_tables(
        mut self,
        tables: Vec<(u32, InteractionTable)>,
    ) -> Result<Self, String> {
        for (guid, table) in tables {
            if self.interaction_tables.insert(guid, table).is_some() {
                return Err("duplicate interaction table owner".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn routines(&self) -> &RoutineStore {
        &self.routines
    }
    pub fn object(&self, guid: u32) -> Option<&ObjectDefinition> {
        self.objects.get(&guid)
    }
    pub fn animation(&self, key: AnimationKey) -> Option<&AnimationMetadata> {
        self.animations.get(&key)
    }
    pub fn animation_by_resource(&self, resource: &str) -> Option<&AnimationMetadata> {
        self.animations
            .values()
            .find(|m| m.resource.eq_ignore_ascii_case(resource))
    }
    pub fn animation_metadata(&self) -> impl Iterator<Item = &AnimationMetadata> {
        self.animations.values()
    }
    pub fn routing_slot(&self, owner: u32, index: u16) -> Option<&RoutingSlot> {
        self.routing_slots.get(&(owner, index))
    }
    pub fn with_routing_slots(
        mut self,
        slots: Vec<((u32, u16), RoutingSlot)>,
    ) -> Result<Self, String> {
        for (key, slot) in slots {
            if self.routing_slots.insert(key, slot).is_some() {
                return Err("duplicate routing slot".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn tuning(&self) -> &TuningSet {
        &self.tuning
    }
    pub fn string(&self, owner: u32, table: u16, index: i32) -> Option<&str> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.strings.get(&(owner, table)).and_then(|s| s.get(i)))
            .map(String::as_str)
    }
    pub fn has_string_table(&self, owner: u32, table: u16) -> bool {
        self.strings.contains_key(&(owner, table))
    }
    pub fn named_tree(&self, owner: u32, name: &str) -> Option<RoutineKey> {
        self.named_trees.get(&(owner, name.to_owned())).copied()
    }
    pub fn suit(&self, owner: u32, scope: u8, index: u16) -> Option<&crate::vm::ResolvedSuit> {
        self.suits.get(&(owner, scope, index))
    }
    pub fn legacy_job_uniform(&self, job_type: i16, level: i16) -> Option<&LegacyJobUniform> {
        self.legacy_job_uniforms.get(&(job_type, level))
    }
    pub fn with_legacy_job_uniforms(
        mut self,
        values: Vec<((i16, i16), LegacyJobUniform)>,
    ) -> Result<Self, String> {
        for (key, value) in values {
            if self.legacy_job_uniforms.insert(key, value).is_some() {
                return Err("duplicate job uniform".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn with_strings(mut self, values: Vec<((u32, u16), Vec<String>)>) -> Result<Self, String> {
        for (key, value) in values {
            if self.strings.insert(key, value).is_some() {
                return Err("duplicate string table".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn with_named_trees(
        mut self,
        values: Vec<((u32, String), RoutineKey)>,
    ) -> Result<Self, String> {
        for (key, value) in values {
            if self.named_trees.insert(key, value).is_some() {
                return Err("duplicate named routine".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn with_suits(
        mut self,
        values: Vec<((u32, u8, u16), crate::vm::ResolvedSuit)>,
    ) -> Result<Self, String> {
        for (key, value) in values {
            if self.suits.insert(key, value).is_some() {
                return Err("duplicate suit resource".into());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.objects.len() > 32767
            || self.animations.len() > 65536
            || self.routines.len() > 65536
            || self.tuning.values.len() > 65536
            || self.routing_slots.len() > 65536
        {
            return Err("content count limit exceeded".into());
        }
        self.routines.validate().map_err(|e| e.to_string())?;
        if self.build_catalog.len() > 65536
            || self
                .build_catalog
                .iter()
                .any(|(id, guid)| *id == 0 || !self.objects.contains_key(guid))
        {
            return Err("invalid build catalog content binding".into());
        }
        for (guid, object) in &self.objects {
            if *guid != object.guid {
                return Err("object GUID mismatch".into());
            }
            object.validate()?;
            if object
                .entry_points
                .values()
                .chain(object.entry_conditions.values())
                .any(|key| self.routines.get(*key).is_none())
            {
                return Err("entrypoint routine is missing".into());
            }
        }
        let limits = crate::interactions::InteractionLimits::default();
        if self.interaction_advertisements.len() > 65536
            || self
                .interaction_advertisements
                .iter()
                .any(|((owner, key), ad)| {
                    ad.motives.len() > 16
                        || ad
                            .motives
                            .iter()
                            .map(|m| m.motive)
                            .collect::<BTreeSet<_>>()
                            .len()
                            != ad.motives.len()
                        || !self
                            .interaction_tables
                            .get(owner)
                            .is_some_and(|table| table.definitions.iter().any(|d| d.key == *key))
                })
        {
            return Err("invalid interaction advertisement binding or bounds".into());
        }
        for (guid, table) in &self.interaction_tables {
            if !self.objects.contains_key(guid) || table.definitions.len() > limits.max_definitions
            {
                return Err("interaction table owner/count invalid".into());
            }
            let mut keys = BTreeSet::new();
            let mut global_seen = false;
            for definition in &table.definitions {
                crate::interactions::query::validate_definition(definition, &limits)
                    .map_err(|e| e.to_string())?;
                if !keys.insert(definition.key) {
                    return Err("duplicate interaction key".into());
                }
                match definition.key.scope {
                    crate::interactions::InteractionScope::Local
                        if !table.local_table_present || global_seen =>
                    {
                        return Err("invalid interaction source order/local presence".into())
                    }
                    crate::interactions::InteractionScope::Global => global_seen = true,
                    _ => {}
                }
                for binding in std::iter::once(definition.action).chain(definition.check) {
                    if self
                        .routines
                        .resolve(binding.code_owner_guid, binding.routine_id)
                        .is_none()
                    {
                        return Err("interaction routine missing".into());
                    }
                }
            }
        }
        let mut resources = BTreeMap::new();
        for animation in self.animations.values() {
            animation
                .validate()
                .map_err(|e| format!("animation: {e:?}"))?;
            if let Some(previous) =
                resources.insert(animation.resource.to_ascii_lowercase(), animation)
            {
                if previous.num_frames != animation.num_frames
                    || previous.time_properties != animation.time_properties
                {
                    return Err("conflicting animation metadata for the same resource".into());
                }
            }
        }
        for slot in self.routing_slots.values() {
            slot.validate()?;
        }
        if self.strings.len() > 65536
            || self.named_trees.len() > 65536
            || self.suits.len() > 65536
            || self.legacy_job_uniforms.len() > 65536
            || self
                .strings
                .values()
                .any(|s| s.len() > 65536 || s.iter().any(|s| s.len() > 65536))
            || self
                .named_trees
                .iter()
                .any(|((_, name), key)| name.len() > 4096 || self.routines.get(*key).is_none())
            || self
                .tuning
                .relationship_multipliers
                .values()
                .any(|v| !v.is_finite())
        {
            return Err("content string, name or tuning bounds".into());
        }
        for uniform in self.legacy_job_uniforms.values() {
            uniform.validate()?;
        }
        for suit in self.suits.values() {
            match suit {
                crate::vm::ResolvedSuit::Reference(reference) => {
                    reference.validate().map_err(|e| format!("{e:?}"))?
                }
                crate::vm::ResolvedSuit::Accessory(name) if name.len() > 16384 => {
                    return Err("accessory name bounds".into())
                }
                _ => {}
            }
        }
        if let Some(tuning) = &self.tuning.tso_motives {
            tuning.validate().map_err(|e| format!("tuning: {e:?}"))?;
        }
        canonical_bytes(self, MAX_CONTENT_BYTES).map(|_| ())
    }
    pub fn descriptor(&self) -> Result<ContentDescriptor, String> {
        let mut descriptor = ContentDescriptor {
            content_hash: hash(&canonical_bytes(
                &(
                    CONTENT_SCHEMA,
                    &self.routines,
                    &self.objects,
                    &self.animations,
                    &self.routing_slots,
                    &self.strings,
                    &self.named_trees,
                    &self.suits,
                    &self.legacy_job_uniforms,
                ),
                MAX_CONTENT_BYTES,
            )?),
            tuning_hash: hash(&canonical_bytes(&self.tuning, MAX_CONTENT_BYTES)?),
        };
        if !self.interaction_tables.is_empty() {
            descriptor.content_hash = hash(&canonical_bytes(
                &(descriptor.content_hash, &self.interaction_tables),
                MAX_CONTENT_BYTES,
            )?);
        }
        if !self.build_catalog.is_empty() {
            descriptor.content_hash = hash(&canonical_bytes(
                &(descriptor.content_hash, &self.build_catalog),
                MAX_CONTENT_BYTES,
            )?);
        }
        if !self.interaction_advertisements.is_empty() {
            descriptor.content_hash = hash(&canonical_bytes(
                &(descriptor.content_hash, &self.interaction_advertisements),
                MAX_CONTENT_BYTES,
            )?);
        }
        Ok(descriptor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecyclePhase {
    Initializing,
    Running,
    Suspended,
    Faulted,
    Exited,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadlineKind {
    Money(i32),
    Balloon {
        icon: Option<EntityRef>,
        index: i8,
        group: u8,
        headline_type: u8,
        flags: u8,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadlineState {
    pub duration: i32,
    pub anim: i32,
    pub kind: HeadlineKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub info: EntityInfo,
    pub attributes: Vec<i16>,
    pub object_data: Vec<i16>,
    pub list: Vec<i16>,
    pub dynamic_sprite_flags: Vec<bool>,
    pub type_attributes: BTreeMap<u16, i16>,
    pub tuning_overrides: BTreeMap<(u16, u16), i16>,
    pub active_advertisements: Option<BTreeMap<(u8, u16), i16>>,
    pub slots: Vec<Option<EntityRef>>,
    pub container: Option<(EntityRef, u16)>,
    pub initial_price: i32,
    pub lockout_started: u64,
    pub avatar: Option<AvatarState>,
    pub lifecycle: LifecyclePhase,
    pub pending_entrypoints: VecDeque<u8>,
    pub main_parameter: ObjectId,
    pub main_stack_object: ObjectId,
    /// Distinct avatars whose B-owned queue's active prefix uses this object.
    pub queued_users: BTreeSet<EntityRef>,
    /// Source RunEveryFrame also covers active headlines/disabled game objects.
    pub always_tick: bool,
    pub disabled_flags: u8,
    pub broken: bool,
    pub headline: Option<HeadlineState>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationLimits {
    pub max_entities: u32,
    pub max_commands_per_tick: u32,
    pub max_list_items: u32,
    pub max_continuations: u32,
    pub instruction_budget_per_entity: u32,
    pub route_search_budget: u32,
    pub max_tick_instructions: u32,
}
impl Default for SimulationLimits {
    fn default() -> Self {
        Self {
            max_entities: 32767,
            max_commands_per_tick: 1024,
            max_list_items: 4096,
            max_continuations: 4096,
            instruction_budget_per_entity: 500001,
            route_search_budget: 1024,
            max_tick_instructions: 4_000_000,
        }
    }
}
impl SimulationLimits {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_entities == 0
            || self.max_entities > 32767
            || self.max_commands_per_tick == 0
            || self.max_commands_per_tick > 65536
            || self.max_list_items == 0
            || self.max_list_items > 65536
            || self.max_continuations == 0
            || self.max_continuations > 32767
            || self.instruction_budget_per_entity == 0
            || self.instruction_budget_per_entity > 500001
            || self.route_search_budget == 0
            || self.route_search_budget > 65536
            || self.max_tick_instructions < self.instruction_budget_per_entity
            || self.max_tick_instructions > 64_000_000
        {
            return Err("simulation limits are invalid".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Preserve the public inline continuation state used by snapshot and replay consumers.
#[allow(clippy::large_enum_variant)]
pub enum ContinuationKind {
    Effect(OperationId),
    Route(RouteContinuation),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeContinuation {
    pub id: u64,
    pub entity: EntityRef,
    pub kind: ContinuationKind,
    pub resumes_vm: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub schema: u16,
    pub mode: VmMode,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub completed_tick: u64,
    pub content: ContentDescriptor,
    pub limits: SimulationLimits,
    pub clock: SimClock,
    pub rng: SimRng,
    pub ids: IdAllocator,
    pub entities: BTreeMap<ObjectId, EntityState>,
    pub threads: BTreeMap<ObjectId, VmThread>,
    pub globals: Vec<i16>,
    pub relationships: crate::vm::RelationshipBook,
    /// Current TS1 family and logical token inventories, supplied by the B importer.
    pub ts1_family_budget: Option<i32>,
    pub ts1_inventory: crate::vm::Ts1InventoryBook,
    pub scheduler: Scheduler,
    pub world: WorldState,
    pub effects: EffectBook,
    pub continuations: BTreeMap<u64, RuntimeContinuation>,
    pub next_continuation: u64,
    pub last_tick_digest: Option<[u8; 32]>,
    pub interaction_queues: BTreeMap<ObjectId, crate::interactions::ActionQueue>,
    pub interaction_access:
        BTreeMap<EntityRef, crate::runtime::interaction_adapter::InteractionAccess>,
}

pub(crate) fn canonical_bytes<T: Serialize>(value: &T, limit: u64) -> Result<Vec<u8>, String> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(limit)
        .serialize(value)
        .map_err(|e| e.to_string())
}
pub(crate) fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
