//! Exact table/fact projection and a deliberately proved read-only check subset.
use super::{entity_ref, MAX_PROJECTION_BYTES};
use crate::{
    budget::ImportBudget,
    content::{ImportedContent, ImportedInteraction},
};
use bincode::Options;
use interaction_rules::{
    adapters::{AuthorityOperation, CheckTreeProvider, WorldProvider},
    ActionFlags, ActorFacts, AvatarPermission, CheckBudget, CheckExit, CheckOutput, CheckRequest,
    CheckState, EntityKey, EntityVersion, Error, InteractionDefinition, InteractionKey,
    InteractionLimits, InteractionScope, InteractionSnapshot, LegacyMode, PermissionFlags,
    PrincipalKey, RoutineBinding, Species, TargetFacts,
};
use sim_core::{
    ids::EntityRef,
    rng::SimRng,
    runtime::{RuntimeRole, SimRuntime},
    snapshot::{self, SnapshotExpectation},
    state::{ContentDescriptor, ContentSet},
    vm::{FrameContext, PrimitiveExit, RoutineKey, VmMode, VmStop},
};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_legacy_formats::Limits;

// VMEntityFlags.Occupied; VMNetInteractionCmd.Verify changes only this raw bit,
// not IsInUse across a group, active avatar frames or the source UseCount.
const OCCUPIED: i16 = 1 << 5;

/// Import reports are trusted in-process outputs of the source/cooked importer,
/// not an authenticated wire format. This catalog rechecks routine bindings and
/// retains raw flags/labels/table absence without interpreting them as authority.
pub struct RuntimeCatalog {
    content: ContentDescriptor,
    objects: BTreeMap<u32, Option<Vec<InteractionDefinition>>>,
    global: BTreeMap<u32, Vec<InteractionDefinition>>,
}
impl RuntimeCatalog {
    /// `global` contains the content owner's global table resolved separately
    /// for each target `code_owner` GUID. An empty slice selects no global
    /// interactions; a binding for one target never supplies another's table.
    pub fn from_imported(
        imported: &ImportedContent,
        global: &[ImportedInteraction],
        limits: &InteractionLimits,
    ) -> Result<Self, String> {
        if imported.objects.len() > 32767 || global.len() > 131072 {
            return Err("interaction catalog count limit".into());
        }
        let mut budget = ImportBudget::new(&Limits {
            max_total_decoded_bytes: MAX_PROJECTION_BYTES,
            ..Limits::default()
        });
        budget.map_entries::<u32, Option<Vec<InteractionDefinition>>>(imported.objects.len())?;
        let mut objects = BTreeMap::new();
        for object in &imported.objects {
            if imported.content.object(object.guid).is_none() || objects.contains_key(&object.guid)
            {
                return Err("missing or duplicate runtime object in interaction catalog".into());
            }
            if object
                .interactions
                .as_ref()
                .is_some_and(|rows| rows.iter().any(|row| row.code_owner != object.guid))
            {
                return Err("local interaction code owner differs from source object".into());
            }
            let entries = object
                .interactions
                .as_ref()
                .map(|rows| {
                    definitions(
                        &imported.content,
                        rows.iter(),
                        InteractionScope::Local,
                        limits,
                        &mut budget,
                    )
                })
                .transpose()?;
            objects.insert(object.guid, entries);
        }
        budget.map_entries::<u32, Vec<&ImportedInteraction>>(global.len())?;
        // Each temporary Vec starts with at most four entries and grows by
        // doubling. Four references per input covers even singleton owners.
        budget.entries::<&ImportedInteraction>(global.len() * 4)?;
        let mut by_owner: BTreeMap<u32, Vec<&ImportedInteraction>> = BTreeMap::new();
        for row in global {
            if !objects.contains_key(&row.code_owner) {
                return Err("global interaction target owner has no imported object".into());
            }
            let rows = by_owner.entry(row.code_owner).or_default();
            if rows.len() >= limits.max_definitions.min(4096) {
                return Err("global interaction definition count limit".into());
            }
            rows.push(row);
        }
        budget.map_entries::<u32, Vec<InteractionDefinition>>(by_owner.len())?;
        let mut global = BTreeMap::new();
        for (owner, rows) in by_owner {
            global.insert(
                owner,
                definitions(
                    &imported.content,
                    rows.iter().copied(),
                    InteractionScope::Global,
                    limits,
                    &mut budget,
                )?,
            );
        }
        Ok(Self {
            content: imported.content.descriptor()?,
            objects,
            global,
        })
    }
    pub fn local(&self, guid: u32) -> Option<Option<&[InteractionDefinition]>> {
        self.objects.get(&guid).map(|value| value.as_deref())
    }
    pub fn global(&self, guid: u32) -> &[InteractionDefinition] {
        self.global.get(&guid).map_or(&[], Vec::as_slice)
    }
}

fn definitions<'a>(
    content: &ContentSet,
    rows: impl Iterator<Item = &'a ImportedInteraction> + Clone,
    scope: InteractionScope,
    limits: &InteractionLimits,
    budget: &mut ImportBudget,
) -> Result<Vec<InteractionDefinition>, String> {
    let count = rows.clone().count();
    if count > limits.max_definitions.min(4096) {
        return Err("interaction definition count limit".into());
    }
    budget.entries::<InteractionDefinition>(count)?;
    budget.map_entries::<u32, ()>(count)?;
    // Validate every row and admit all label storage before cloning any label.
    let mut indices = BTreeSet::new();
    for row in rows.clone() {
        if !indices.insert(row.tta_index) {
            return Err("duplicate TTAIndex in interaction catalog".into());
        }
        if row
            .label
            .as_ref()
            .is_some_and(|label| label.len() > limits.max_label_bytes.min(65536))
        {
            return Err("interaction label byte limit".into());
        }
        budget.reserve(row.label.as_ref().map_or(0, String::len))?;
        for key in std::iter::once(row.action).chain(row.check) {
            if key.id == 0 || content.routines().resolve(row.code_owner, key.id) != Some(key) {
                return Err("interaction routine binding differs from runtime namespace".into());
            }
        }
    }
    Ok(rows
        .map(|row| InteractionDefinition {
            key: InteractionKey {
                tta_index: row.tta_index,
                scope,
            },
            action: RoutineBinding {
                routine_id: row.action.id,
                code_owner_guid: row.code_owner,
            },
            check: row.check.map(|key| RoutineBinding {
                routine_id: key.id,
                code_owner_guid: row.code_owner,
            }),
            flags: ActionFlags(row.flags),
            permissions: PermissionFlags(row.permissions),
            label: row.label.clone(),
        })
        .collect())
}

/// The service owner supplies authenticated principal operations and the actual
/// object/donated-object ownership projection absent from A's public SimState.
/// Implementations must use a captured authoritative view, never client claims.
pub trait InteractionAuthority {
    fn authorize(
        &self,
        principal: PrincipalKey,
        actor: EntityKey,
        operation: AuthorityOperation,
    ) -> bool;
    /// None is unresolved ownership and fails TSO avatar/game-object snapshots.
    fn owns_target(&self, actor: EntityRef, target: EntityRef) -> Option<bool>;
}

pub struct RuntimeInteractionWorld<'a, A: InteractionAuthority> {
    runtime: &'a SimRuntime,
    catalog: &'a RuntimeCatalog,
    authority: &'a A,
    revision: u64,
}
impl<'a, A: InteractionAuthority> RuntimeInteractionWorld<'a, A> {
    /// The integrating authority advances `revision` for every query-visible VM
    /// or access-policy change, including changes between accepted ticks. The
    /// runtime and authority view stay borrowed and immutable during each query.
    pub fn new(
        runtime: &'a SimRuntime,
        catalog: &'a RuntimeCatalog,
        authority: &'a A,
        revision: u64,
    ) -> Result<Self, String> {
        if revision == 0 {
            return Err("interaction world revision must be nonzero".into());
        }
        if runtime.state().content != catalog.content {
            return Err("interaction catalog belongs to different runtime content".into());
        }
        Ok(Self {
            runtime,
            catalog,
            authority,
            revision,
        })
    }
}

impl<A: InteractionAuthority> WorldProvider for RuntimeInteractionWorld<'_, A> {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn entity_version(&self, key: EntityKey) -> Option<EntityVersion> {
        let reference = entity_ref(key).ok()?;
        if !self.runtime.state().ids.is_live(reference) {
            return None;
        }
        let item = self.runtime.state().entities.get(&reference.object_id)?;
        if item.info.reference != reference || item.info.dead {
            return None;
        }
        Some(EntityVersion {
            key,
            revision: item.revision,
        })
    }
    fn authorize(
        &self,
        principal: PrincipalKey,
        actor: EntityKey,
        operation: AuthorityOperation,
    ) -> bool {
        self.entity_version(actor).is_some()
            && self.authority.authorize(principal, actor, operation)
    }
    fn snapshot(
        &self,
        actor: EntityKey,
        target: EntityKey,
        limits: &InteractionLimits,
    ) -> interaction_rules::Result<InteractionSnapshot> {
        let actor_version = self
            .entity_version(actor)
            .ok_or(Error::StaleEntity(actor))?;
        let target_version = self
            .entity_version(target)
            .ok_or(Error::StaleEntity(target))?;
        let actor_ref = entity_ref(actor).map_err(|_| Error::StaleEntity(actor))?;
        let target_ref = entity_ref(target).map_err(|_| Error::StaleEntity(target))?;
        let state = self.runtime.state();
        let caller = &state.entities[&actor_ref.object_id];
        let callee = &state.entities[&target_ref.object_id];
        let local = self
            .catalog
            .local(callee.info.guid)
            .ok_or(Error::Unsupported(
                "target has no resolved interaction catalog",
            ))?;
        let global = self.catalog.global(callee.info.guid);
        if local
            .map_or(0, <[InteractionDefinition]>::len)
            .saturating_add(global.len())
            > limits.max_definitions
        {
            return Err(Error::LimitExceeded("interaction definitions"));
        }
        // Per-request limits may be narrower than those used to build the
        // catalog. Admit every retained label before the snapshot clones one.
        if local.into_iter().flatten().chain(global).any(|definition| {
            definition
                .label
                .as_ref()
                .is_some_and(|label| label.len() > limits.max_label_bytes)
        }) {
            return Err(Error::LimitExceeded("interaction label bytes"));
        }
        let mode = if state.mode == VmMode::Ts1 {
            LegacyMode::Ts1
        } else {
            LegacyMode::Tso
        };
        let owns_target =
            if mode == LegacyMode::Tso && caller.info.is_avatar && !callee.info.is_avatar {
                self.authority
                    .owns_target(actor_ref, target_ref)
                    .ok_or(Error::Unsupported(
                        "authoritative TSO object ownership projection missing",
                    ))?
            } else {
                false
            };
        let avatar = caller.avatar.as_ref();
        let actor_facts = ActorFacts {
            version: actor_version,
            is_avatar: caller.info.is_avatar,
            species: avatar.map_or(Species::Human, |avatar| {
                if avatar.is_cat() {
                    Species::Cat
                } else if avatar.is_dog() {
                    Species::Dog
                } else {
                    Species::Human
                }
            }),
            permission: match avatar.map_or(0, |avatar| avatar.permissions as u8) {
                0 => AvatarPermission::Visitor,
                1 => AvatarPermission::Roommate,
                2 => AvatarPermission::BuildBuyRoommate,
                3 => AvatarPermission::Owner,
                _ => AvatarPermission::Admin,
            },
            carrying: caller.slots.first().is_some_and(Option::is_some),
            ghost: avatar.is_some_and(|avatar| avatar.person_data.values[68] > 0),
            owns_target,
            ts1_ungreeted_visitor: avatar.is_some_and(|avatar| {
                avatar.person_data.values[32] == 1 && avatar.person_data.values[34] < 2
            }),
            age: avatar.map_or(0, |avatar| avatar.person_data.values[58]),
        };
        let base = state
            .entities
            .get(&callee.info.base_object)
            .ok_or(Error::RuntimeFailure("target base object is missing"))?;
        let target_facts = TargetFacts {
            version: target_version,
            is_game_object: !callee.info.is_avatar,
            broken: base.broken,
            disabled: callee.disabled_flags != 0,
        };
        let thread = &state.threads[&actor_ref.object_id];
        let mut checked = CheckState::default();
        checked.rng_seed = state.rng.state();
        checked.temp_registers = thread.temps;
        checked.temp_xl = thread.temp_xl;
        checked.hidden = caller.object_data[34] == 1;
        checked.hide_interaction = caller.object_data[50] == 1;
        checked.out_of_world = caller.info.position.x == i16::MIN
            && caller.info.position.y == i16::MIN
            && caller.info.position.level == 1;
        checked.target_occupied = callee.object_data[8] & OCCUPIED != 0;
        let size = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .serialized_size(state)
            .map_err(|_| Error::RuntimeFailure("runtime snapshot size failed"))?;
        let total = size
            .checked_add((snapshot::SNAPSHOT_HEADER_LEN + snapshot::SNAPSHOT_CHECKSUM_LEN) as u64)
            .ok_or(Error::LimitExceeded("check provider state bytes"))?;
        if total > limits.max_provider_state_bytes.min(MAX_PROJECTION_BYTES) as u64 {
            return Err(Error::LimitExceeded("check provider state bytes"));
        }
        let bytes = self
            .runtime
            .snapshot()
            .map_err(|_| Error::RuntimeFailure("runtime snapshot capture failed"))?;
        checked.set_provider_state_owned(bytes, limits)?;
        let mut snapshot = InteractionSnapshot::new(
            mode,
            self.revision,
            actor_facts,
            target_facts,
            checked,
            limits,
        )?;
        snapshot.set_local_table_present(local.is_some())?;
        for definition in local.into_iter().flatten().chain(global) {
            snapshot.add_definition(definition.clone(), limits)?;
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ReadOnlyCertificate {
    pub routines: usize,
    pub instructions: usize,
}

/// Certify EVERY instruction in the complete direct-call closure, including
/// currently unreachable branches. Only Expression comparisons, Test Object
/// Type and direct calls are admitted. All state writes, RNG, advertisements, action strings,
/// unknown primitives, indirect calls and host requests fail before execution.
pub fn certify_read_only(
    content: &ContentSet,
    code_owner: u32,
    routine_id: u16,
) -> Result<ReadOnlyCertificate, String> {
    let root = content
        .routines()
        .resolve(code_owner, routine_id)
        .ok_or("check routine is unresolved")?;
    certify_root(content, code_owner, root)
}

// The root binding may differ from the action-frame CodeOwner. The original
// CheckAction carries those two values separately; nested calls keep CodeOwner.
fn certify_root(
    content: &ContentSet,
    code_owner: u32,
    root: RoutineKey,
) -> Result<ReadOnlyCertificate, String> {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    let mut instructions = 0usize;
    while let Some(key) = pending.pop() {
        if seen.contains(&key) {
            continue;
        }
        if seen.len() >= 512 {
            return Err("read-only proof routine limit".into());
        }
        seen.insert(key);
        let routine = content
            .routines()
            .get(key)
            .ok_or("check routine disappeared")?;
        instructions = instructions
            .checked_add(routine.instructions().len())
            .filter(|count| *count <= 131072)
            .ok_or("read-only proof instruction limit")?;
        for (index, instruction) in routine.instructions().iter().enumerate() {
            if instruction.opcode >= 256 {
                let call = content
                    .routines()
                    .resolve(code_owner, instruction.opcode)
                    .ok_or_else(|| {
                        format!(
                            "unresolved check call {} at {}:{index}",
                            instruction.opcode, key.id
                        )
                    })?;
                if !seen.contains(&call) && !pending.contains(&call) {
                    if pending.len() + seen.len() >= 512 {
                        return Err("read-only proof routine limit".into());
                    }
                    pending.push(call);
                }
            } else if !(instruction.opcode == 32
                || (instruction.opcode == 2
                    && [0, 1, 2, 8, 14, 15, 16].contains(&instruction.operand[5])))
            {
                return Err(format!(
                    "check {}:{index} opcode {} requires full query-state capability",
                    key.id, instruction.opcode
                ));
            }
        }
    }
    Ok(ReadOnlyCertificate {
        routines: seen.len(),
        instructions,
    })
}

/// Executes trusted authoritative definitions supplied by the WorldProvider.
/// RuntimeCatalog binds source action owners to target GUIDs; another provider
/// must establish its own effective-content binding. This evaluator preserves
/// separately resolved check roots and action-frame CodeOwner and does not grant
/// authorization or infer either owner from a client-supplied target identity.
pub struct ReadOnlyChecks<'a> {
    content: &'a ContentSet,
    expectation: SnapshotExpectation,
}
impl<'a> ReadOnlyChecks<'a> {
    pub fn new(content: &'a ContentSet, lot_id: u64, authority_epoch: u64) -> Self {
        let mut expectation = SnapshotExpectation::new(lot_id, authority_epoch);
        expectation.limits.max_payload_bytes = MAX_PROJECTION_BYTES as u64;
        Self {
            content,
            expectation,
        }
    }
}
impl CheckTreeProvider for ReadOnlyChecks<'_> {
    fn evaluate(
        &self,
        request: CheckRequest<'_>,
        state: &mut CheckState,
        _output: &mut CheckOutput,
        budget: &mut CheckBudget,
    ) -> interaction_rules::Result<CheckExit> {
        let check = request
            .definition
            .check
            .ok_or(Error::InvalidSnapshot("missing check binding"))?;
        let owner = request.definition.action.code_owner_guid;
        let routine = self
            .content
            .routines()
            .resolve(check.code_owner_guid, check.routine_id)
            .ok_or(Error::InvalidSnapshot("check routine missing"))?;
        certify_root(self.content, owner, routine).map_err(|_| {
            Error::Unsupported(
                "check requires full detached query-state capability; read-only proof rejected",
            )
        })?;
        if state.provider_state().len() > MAX_PROJECTION_BYTES {
            return Err(Error::LimitExceeded("check provider state bytes"));
        }
        let mut stored = snapshot::decode(state.provider_state(), self.content, self.expectation)
            .map_err(|_| Error::InvalidSnapshot("invalid runtime check snapshot"))?;
        let actor = entity_ref(request.actor).map_err(|_| Error::StaleEntity(request.actor))?;
        let target = entity_ref(request.target).map_err(|_| Error::StaleEntity(request.target))?;
        for (reference, key) in [(actor, request.actor), (target, request.target)] {
            if !stored.ids.is_live(reference) {
                return Err(Error::StaleEntity(key));
            }
        }
        let mode = if stored.mode == VmMode::Ts1 {
            LegacyMode::Ts1
        } else {
            LegacyMode::Tso
        };
        if mode != request.mode {
            return Err(Error::InvalidSnapshot("check runtime mode differs"));
        }
        // B resets HideInteraction for every menu entry; represent that exact
        // raw source write in the detached runtime before the comparison reads.
        stored
            .entities
            .get_mut(&actor.object_id)
            .expect("validated actor")
            .object_data[50] = i16::from(state.hide_interaction);
        // Intent validation supplies false for this flag, then discards the
        // detached result. Preserve every other flag, all queue users and the
        // wider in-use facts. Ordinary UI/tick checks carry the captured bit.
        let flags = &mut stored
            .entities
            .get_mut(&target.object_id)
            .expect("validated target")
            .object_data[8];
        *flags = (*flags & !OCCUPIED) | if state.target_occupied { OCCUPIED } else { 0 };
        let thread = stored
            .threads
            .get_mut(&actor.object_id)
            .expect("validated thread");
        thread.temps = state.temp_registers;
        thread.temp_xl = state.temp_xl;
        stored.rng = SimRng::new(state.rng_seed);
        let limit = budget
            .remaining()
            .min(u64::from(stored.limits.instruction_budget_per_entity));
        if limit == 0 {
            return Err(Error::LimitExceeded("check instruction budget"));
        }
        let runtime = SimRuntime::from_state(stored, self.content.clone(), RuntimeRole::Replica)
            .map_err(|_| Error::InvalidSnapshot("detached check state invalid"))?;
        let context = FrameContext {
            caller: actor,
            callee: target,
            stack_object: target.object_id,
            stack_object_ref: Some(target),
            code_owner: owner,
        };
        let result = runtime
            .query_behavior(actor, routine, context, request.args.to_vec(), limit as u32)
            .map_err(|_| Error::RuntimeFailure("read-only runtime check failed"))?;
        budget.spend(u64::from(result.instructions))?;
        if result.temps != state.temp_registers || result.temp_xl != state.temp_xl {
            return Err(Error::RuntimeFailure(
                "certified read-only check changed registers",
            ));
        }
        match result.stop {
            VmStop::Completed(PrimitiveExit::ReturnTrue) => Ok(CheckExit::ReturnTrue),
            VmStop::Completed(PrimitiveExit::ReturnFalse) => Ok(CheckExit::ReturnFalse),
            _ => Ok(CheckExit::Aborted),
        }
    }
}
