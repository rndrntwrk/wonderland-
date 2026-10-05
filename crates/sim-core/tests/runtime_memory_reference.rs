//! Source-backed runtime memory and failure-atomic projection regressions.
use sim_core::avatars::{AvatarPlatform, AvatarState};
use sim_core::clock::SimClock;
use sim_core::effects::{EffectBook, EffectLimits};
use sim_core::ids::{EntityRef, IdAllocator, ObjectId, PersistentId};
use sim_core::rng::SimRng;
use sim_core::runtime_memory::{
    read_memory, read_memory_with_active, sync_projection, write_memory, write_memory_with_active,
    MemorySignal, MAX_ATTRIBUTES,
};
use sim_core::scheduler::Scheduler;
use sim_core::state::{
    ContentSet, EntityState, LifecyclePhase, ObjectDefinition, SimState, SimulationLimits,
    TuningSet, SIMULATION_SCHEMA,
};
use sim_core::vm::{
    ClockKind, EntityField, EntityInfo, FrameContext, MemoryAddress, RoutineKey, RoutineScope,
    RoutineStore, SpecialResult, VmFault, VmFrame, VmMode, VmPosition, VmThread,
};
use sim_core::world::{Facing, LotModel, LotPosition, TilePos, WorldObject, WorldState};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn address(entity: EntityRef, field: EntityField, index: u16) -> MemoryAddress {
    MemoryAddress::Entity {
        entity,
        field,
        index,
    }
}

fn fixture() -> (SimState, ContentSet, [EntityRef; 3]) {
    let mut routines = RoutineStore::new();
    routines.bind_semiglobal(30, 2000).unwrap();
    let mut definitions = Vec::new();
    for guid in [10, 20, 30] {
        let mut definition = ObjectDefinition::new(guid, 0);
        definition.definition = vec![123, -1, 16, guid as i16];
        if guid == 10 {
            definition.master_definition = vec![-777, 999];
        }
        if guid == 30 {
            definition.entry_point_count = 40;
        }
        definition.object_data[4] = 1;
        definition.object_data[42] = 3;
        definition.slot_count = if guid == 20 { 3 } else { 2 };
        definitions.push(definition);
    }
    let mut tuning = TuningSet::default();
    tuning.motive_limits[5] = 150;
    tuning.values = BTreeMap::from([
        ((10, 4096, 5), 111),
        ((30, 4096, 5), 333),
        ((2000, 8192, 5), 444),
        ((0, 256, 5), 555),
        ((999, 8192, 5), 666),
    ]);
    let content = ContentSet::new(routines, definitions, Vec::new(), tuning).unwrap();
    let mut ids = IdAllocator::new();
    let references = [
        ids.allocate().unwrap(),
        ids.allocate().unwrap(),
        ids.allocate().unwrap(),
    ];
    let mut entities = BTreeMap::new();
    let mut threads = BTreeMap::new();
    let mut world = WorldState::default();
    for (index, reference) in references.into_iter().enumerate() {
        let guid = ((index + 1) * 10) as u32;
        let is_avatar = index == 1;
        let position = VmPosition {
            x: 24 + index as i16 * 32,
            y: 24,
            level: 1,
        };
        let definition = content.object(guid).unwrap();
        let info = EntityInfo {
            reference,
            guid,
            master_guid: None,
            semiglobal: None,
            persistent_id: if is_avatar { 123 } else { 0 },
            is_avatar,
            dead: false,
            category: 0,
            family: 0,
            position,
            direction: 0,
            level_offset: 0,
            base_object: reference.object_id,
            multi_tile: false,
            group: vec![reference.object_id],
        };
        entities.insert(
            reference.object_id,
            EntityState {
                info,
                attributes: Vec::new(),
                object_data: definition.object_data.clone(),
                list: Vec::new(),
                dynamic_sprite_flags: vec![false; 128],
                type_attributes: BTreeMap::new(),
                tuning_overrides: BTreeMap::new(),
                active_advertisements: None,
                slots: vec![None; usize::from(definition.slot_count)],
                container: None,
                initial_price: 500,
                lockout_started: 0,
                avatar: is_avatar
                    .then(|| AvatarState::new(reference, PersistentId(123), AvatarPlatform::Tso)),
                lifecycle: LifecyclePhase::Running,
                pending_entrypoints: VecDeque::new(),
                main_parameter: ObjectId::NULL,
                main_stack_object: ObjectId::NULL,
                queued_users: BTreeSet::new(),
                always_tick: is_avatar,
                disabled_flags: 0,
                broken: false,
                headline: None,
                revision: 0,
            },
        );
        threads.insert(reference.object_id, VmThread::new(reference, VmMode::Tso));
        let mut projection = WorldObject::new(
            reference,
            LotPosition::new(i32::from(position.x), i32::from(position.y), 1),
        );
        projection.rules.is_avatar = is_avatar;
        world.insert_object(projection).unwrap();
    }
    let state = SimState {
        schema: SIMULATION_SCHEMA,
        mode: VmMode::Tso,
        lot_id: 1,
        authority_epoch: 1,
        completed_tick: 0,
        content: content.descriptor().unwrap(),
        limits: SimulationLimits::default(),
        clock: SimClock::new(false, 0),
        rng: SimRng::new(1),
        ids,
        entities,
        threads,
        globals: vec![0; 38],
        scheduler: Scheduler::new(0),
        world,
        effects: EffectBook::new(1, EffectLimits::default()).unwrap(),
        relationships: Default::default(),
        ts1_family_budget: None,
        ts1_inventory: sim_core::vm::Ts1InventoryBook::default(),
        continuations: BTreeMap::new(),
        next_continuation: 1,
        last_tick_digest: None,
    };
    (state, content, references)
}

fn read(
    state: &SimState,
    content: &ContentSet,
    entity: EntityRef,
    field: EntityField,
    index: u16,
) -> i16 {
    read_memory(state, content, &address(entity, field, index)).unwrap()
}

#[test]
fn computed_object_data_keeps_source_shadow_write_semantics() {
    let (mut state, content, [object, avatar, _]) = fixture();
    for (index, value) in [
        (11, 999),
        (2, 777),
        (3, 555),
        (26, 333),
        (62, 222),
        (67, 111),
    ] {
        assert!(
            write_memory(
                &mut state,
                &content,
                &address(object, EntityField::ObjectData, index),
                value
            )
            .unwrap()
            .written
        );
        assert_eq!(
            state.entities[&object.object_id].object_data[usize::from(index)],
            value
        );
    }
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 11),
        object.object_id.0
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 2),
        0
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 3),
        -1
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 62),
        0
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 67),
        2
    );
    state.entities.get_mut(&object.object_id).unwrap().container = Some((avatar, 2));
    state.entities.get_mut(&avatar.object_id).unwrap().slots[2] = Some(object);
    sync_projection(&mut state, object).unwrap();
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 2),
        avatar.object_id.0
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 26),
        avatar.object_id.0
    );
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 3),
        2
    );
    assert_eq!(
        state.entities[&object.object_id].info.position,
        state.entities[&avatar.object_id].info.position
    );
    assert!(state.world.object(object).unwrap().rects().is_empty());
    state.entities.get_mut(&object.object_id).unwrap().container = None;
    state
        .entities
        .get_mut(&object.object_id)
        .unwrap()
        .info
        .position = VmPosition {
        x: i16::MIN,
        y: i16::MIN,
        level: 1,
    };
    sync_projection(&mut state, object).unwrap();
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 29),
        -5
    );
    assert!(state
        .world
        .object(object)
        .unwrap()
        .position
        .is_out_of_world());
}

#[test]
fn dynamic_sprite_indices_alias_the_second_u64_using_csharp_shift_masks() {
    let (mut state, content, [object, _, _]) = fixture();
    let field = EntityField::DynamicSpriteFlag;
    write_memory(&mut state, &content, &address(object, field, 0), 1).unwrap();
    write_memory(&mut state, &content, &address(object, field, 128), 7).unwrap();
    write_memory(&mut state, &content, &address(object, field, u16::MAX), 1).unwrap();
    for index in [0, 64, 128, 192, 127, 191, u16::MAX] {
        assert_eq!(read(&state, &content, object, field, index), 1);
    }
    assert_eq!(read(&state, &content, object, field, 63), 0);
    write_memory(&mut state, &content, &address(object, field, 192), -1).unwrap();
    assert_eq!(read(&state, &content, object, field, 64), 0);
    assert_eq!(read(&state, &content, object, field, 0), 1);
}

#[test]
fn attributes_grow_with_zero_fill_and_bounded_writes_fail_atomically() {
    let (mut state, content, [object, _, _]) = fixture();
    assert_eq!(
        read(&state, &content, object, EntityField::Attribute, u16::MAX),
        0
    );
    write_memory(
        &mut state,
        &content,
        &address(object, EntityField::Attribute, 12),
        77,
    )
    .unwrap();
    assert_eq!(
        state.entities[&object.object_id].attributes,
        [vec![0; 12], vec![77]].concat()
    );
    write_memory(
        &mut state,
        &content,
        &address(object, EntityField::Attribute, (MAX_ATTRIBUTES - 1) as u16),
        -9,
    )
    .unwrap();
    let before = state.clone();
    assert!(matches!(
        write_memory(
            &mut state,
            &content,
            &address(object, EntityField::Attribute, MAX_ATTRIBUTES as u16),
            1
        ),
        Err(VmFault::Bounds { .. })
    ));
    assert_eq!(state, before);
}

#[test]
fn flag_direction_and_placement_writes_update_the_projection_atomically() {
    let (mut state, content, [object, avatar, _]) = fixture();
    let output = write_memory(
        &mut state,
        &content,
        &address(object, EntityField::ObjectData, 1),
        -1,
    )
    .unwrap();
    assert!(output
        .signals
        .contains(&MemorySignal::ProjectionChanged { entity: object }));
    assert_eq!(state.entities[&object.object_id].info.direction, 7);
    assert_eq!(state.world.object(object).unwrap().facing, Facing(7));
    write_memory(
        &mut state,
        &content,
        &address(object, EntityField::ObjectData, 8),
        (0x8000_u16 | 2 | 4 | 16 | 32) as i16,
    )
    .unwrap();
    let rules = &state.world.object(object).unwrap().rules;
    assert!(
        rules.zero_extent && rules.allow_person_intersection && rules.disallow_person_intersection
    );
    assert!(state.world.object(object).unwrap().in_use);
    for index in [4, 13, 42] {
        write_memory(
            &mut state,
            &content,
            &address(object, EntityField::ObjectData, index),
            -1,
        )
        .unwrap();
    }
    let rules = &state.world.object(object).unwrap().rules;
    assert_eq!(
        (rules.allowed_heights, rules.wall_flags, rules.flags),
        (65535, 0x0fff, 0x3fff)
    );
    assert_eq!(state.entities[&object.object_id].object_data[42], -1);
    let reset = write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::ObjectData, 8),
        512,
    )
    .unwrap();
    assert!(reset
        .signals
        .contains(&MemorySignal::ResetRequested { entity: avatar }));
    state.entities.get_mut(&object.object_id).unwrap().revision = u64::MAX;
    let before = state.clone();
    assert!(matches!(
        write_memory(
            &mut state,
            &content,
            &address(object, EntityField::ObjectData, 1),
            2
        ),
        Err(VmFault::Arithmetic(_))
    ));
    assert_eq!(state, before);
}

#[test]
fn a_rejected_world_projection_cannot_leave_new_vm_memory() {
    let (mut state, content, [object, _, _]) = fixture();
    state.world.remove_object(object).unwrap();
    let before = state.clone();
    assert!(write_memory(
        &mut state,
        &content,
        &address(object, EntityField::ObjectData, 8),
        4
    )
    .is_err());
    assert_eq!(state, before);
}

#[test]
fn pending_build_lock_rejects_projection_writes_without_partial_memory_changes() {
    use sim_core::world::build::{BuildAuthority, BuildEdit, BuildIntent};
    let (mut state, content, [object, avatar, _]) = fixture();
    let mut authority = BuildAuthority::new(avatar, PersistentId(123), 1000);
    authority.can_build = true;
    authority.can_manage_others = true;
    let intent = BuildIntent::new(
        1,
        avatar,
        state.world.lot.revision().architecture,
        vec![BuildEdit::MoveObject {
            entity: object,
            expected_revision: state.world.object(object).unwrap().revision,
            position: LotPosition::new(152, 152, 1),
            facing: Facing::NORTH,
        }],
    );
    let mut scripts = |_: &sim_core::world::IntersectionCall| true;
    let preview = state
        .world
        .preview_build(intent, &authority, &mut scripts)
        .unwrap();
    state.world.begin_build_commit(preview, &authority).unwrap();
    let before = state.clone();
    let result = write_memory(
        &mut state,
        &content,
        &address(object, EntityField::ObjectData, 1),
        2,
    );
    assert!(
        matches!(result, Err(VmFault::InvalidContent(detail)) if detail.contains("BuildLocked"))
    );
    assert_eq!(state, before);
    assert_eq!(state.world.builds.pending_operation(), Some(1));
}

#[test]
fn definition_values_and_explicit_function_lengths_preserve_empty_trailing_slots() {
    let (state, content, [object, _, other]) = fixture();
    assert_eq!(
        read(&state, &content, object, EntityField::Definition, 0),
        123
    );
    assert_eq!(
        read(&state, &content, object, EntityField::Definition, 3),
        10
    );
    assert_eq!(
        read(&state, &content, object, EntityField::MasterDefinition, 0),
        -777
    );
    assert_eq!(read(&state, &content, object, EntityField::Function, 32), 0);
    assert!(read_memory(
        &state,
        &content,
        &address(object, EntityField::Function, 33)
    )
    .is_err());
    assert_eq!(read(&state, &content, other, EntityField::Function, 39), 0);
    assert!(read_memory(&state, &content, &address(other, EntityField::Function, 40)).is_err());
}

#[test]
fn skill_lock_read_uses_actual_thread_presence_including_the_active_view() {
    let (mut state, content, [_, avatar, _]) = fixture();
    state
        .entities
        .get_mut(&avatar.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .skill_mode = 2;
    let target = address(avatar, EntityField::PersonData, 70);
    assert_eq!(read_memory(&state, &content, &target), Ok(0x7fff));
    let active = state.threads.remove(&avatar.object_id).unwrap();
    assert_eq!(read_memory(&state, &content, &target), Ok(0));
    state
        .entities
        .get_mut(&avatar.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .has_thread = false;
    assert_eq!(
        read_memory_with_active(&state, &content, &target, Some(&active)),
        Ok(0x7fff)
    );
}

#[test]
fn group_price_updates_are_shared_and_exhaustion_does_not_partially_commit() {
    let (mut state, content, [object, _, other]) = fixture();
    for reference in [object, other] {
        let value = state.entities.get_mut(&reference.object_id).unwrap();
        value.info.group = vec![object.object_id, other.object_id];
        value.info.base_object = object.object_id;
        value.info.multi_tile = true;
    }
    state
        .entities
        .get_mut(&object.object_id)
        .unwrap()
        .initial_price = 65535;
    assert_eq!(
        read(&state, &content, other, EntityField::ObjectData, 41),
        -1
    );
    write_memory(
        &mut state,
        &content,
        &address(other, EntityField::ObjectData, 41),
        -25,
    )
    .unwrap();
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 41),
        -25
    );
    assert_eq!(
        read(&state, &content, other, EntityField::ObjectData, 41),
        -25
    );
    assert_eq!(state.entities[&object.object_id].object_data[41], 0);
    assert_eq!(state.entities[&other.object_id].object_data[41], -25);
    state.entities.get_mut(&other.object_id).unwrap().revision = u64::MAX;
    let before = state.clone();
    assert!(write_memory(
        &mut state,
        &content,
        &address(object, EntityField::ObjectData, 41),
        10
    )
    .is_err());
    assert_eq!(state, before);
}

#[test]
fn lockout_counts_use_simulation_tick_and_the_removed_active_thread() {
    let (mut state, content, [object, _, _]) = fixture();
    state.scheduler = Scheduler::new(10);
    let target = address(object, EntityField::ObjectData, 25);
    write_memory(&mut state, &content, &target, 8).unwrap();
    state.scheduler = Scheduler::new(11);
    assert_eq!(read_memory(&state, &content, &target), Ok(7));
    state.scheduler = Scheduler::new(19);
    assert_eq!(read_memory(&state, &content, &target), Ok(0));
    let active = state.threads.remove(&object.object_id).unwrap();
    write_memory_with_active(&mut state, &content, &target, 3, Some(&active)).unwrap();
    assert_eq!(state.entities[&object.object_id].lockout_started, 19);
    assert_eq!(
        read_memory_with_active(&state, &content, &target, Some(&active)),
        Ok(3)
    );
    state
        .entities
        .get_mut(&object.object_id)
        .unwrap()
        .lockout_started = 20;
    assert!(read_memory_with_active(&state, &content, &target, Some(&active)).is_err());
}

#[test]
fn globals_and_tuning_distinguish_stored_slots_callee_overrides_and_code_owner() {
    let (mut state, content, [object, _, _]) = fixture();
    state.clock.hours = 12;
    write_memory(&mut state, &content, &MemoryAddress::Global(0), 7).unwrap();
    assert_eq!(state.globals[0], 7);
    assert_eq!(
        read_memory(&state, &content, &MemoryAddress::Global(0)),
        Ok(12)
    );
    write_memory(&mut state, &content, &MemoryAddress::Global(37), -22).unwrap();
    assert_eq!(
        read_memory(&state, &content, &MemoryAddress::Global(37)),
        Ok(-22)
    );
    assert!(read_memory(&state, &content, &MemoryAddress::Global(38)).is_err());
    let tuning = |mode, owner, table| MemoryAddress::Tuning {
        callee: object,
        code_owner: owner,
        table_id: table,
        key_id: 5,
        resource_mode: mode,
    };
    assert_eq!(read_memory(&state, &content, &tuning(0, 30, 4096)), Ok(333));
    state
        .entities
        .get_mut(&object.object_id)
        .unwrap()
        .tuning_overrides
        .insert((4096, 5), -777);
    assert_eq!(
        read_memory(&state, &content, &tuning(0, 30, 4096)),
        Ok(-777)
    );
    assert_eq!(read_memory(&state, &content, &tuning(1, 30, 8192)), Ok(444));
    assert_eq!(
        read_memory(&state, &content, &tuning(1, 999, 8192)),
        Ok(666)
    );
    assert_eq!(read_memory(&state, &content, &tuning(2, 30, 256)), Ok(555));
    assert_eq!(
        read_memory(&state, &content, &MemoryAddress::MotiveLimit(5)),
        Ok(150)
    );
    for (index, expected) in [(0, 0), (1, 0), (2, 0), (3, 1), (4, 1), (5, 1), (6, 0)] {
        assert_eq!(
            read_memory(
                &state,
                &content,
                &MemoryAddress::Clock {
                    kind: ClockKind::Standard,
                    index
                }
            ),
            Ok(expected)
        );
    }
    assert_eq!(
        read_memory(
            &state,
            &content,
            &MemoryAddress::Clock {
                kind: ClockKind::Game,
                index: 6
            }
        ),
        Ok(1997)
    );
}

#[test]
fn avatar_setters_preserve_successful_noops_and_emit_only_requests() {
    let (mut state, content, [_, avatar, _]) = fixture();
    {
        let value = state
            .entities
            .get_mut(&avatar.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap();
        value.skill_mode = 2;
        value.person_data.values[10] = 7;
        value.person_data.values[70] = 4;
        value.person_data.values[74] = 55;
    }
    for (index, value) in [(10, 99), (70, 88), (74, 9), (92, 3)] {
        assert!(
            write_memory(
                &mut state,
                &content,
                &address(avatar, EntityField::PersonData, index),
                value
            )
            .unwrap()
            .written
        );
    }
    let value = state.entities[&avatar.object_id].avatar.as_ref().unwrap();
    assert_eq!(
        (
            value.person_data.values[10],
            value.person_data.values[70],
            value.person_data.values[74],
            value.display_flags
        ),
        (7, 4, 55, 9)
    );
    let before = state.clone();
    assert!(
        !write_memory(
            &mut state,
            &content,
            &address(avatar, EntityField::PersonData, 91),
            6
        )
        .unwrap()
        .written
    );
    assert_eq!(state, before);
    let money = write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 1),
        250,
    )
    .unwrap();
    assert_eq!(
        money.signals,
        vec![MemorySignal::MoneyHeadline {
            entity: avatar,
            amount: 250
        }]
    );
    assert_eq!(
        state.entities[&avatar.object_id]
            .avatar
            .as_ref()
            .unwrap()
            .budget_mirror,
        0
    );
    assert!(write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 1),
        i16::MIN
    )
    .unwrap()
    .signals
    .is_empty());
    assert!(write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 33),
        9
    )
    .unwrap()
    .signals
    .contains(&MemorySignal::QueueDirty { entity: avatar }));
    write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 91),
        -1,
    )
    .unwrap();
    write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 92),
        3,
    )
    .unwrap();
    assert_eq!(
        read(&state, &content, avatar, EntityField::PersonData, 92),
        3
    );
    state.mode = VmMode::Ts1;
    state.entities.get_mut(&avatar.object_id).unwrap().avatar = Some(AvatarState::new(
        avatar,
        PersistentId(123),
        AvatarPlatform::Ts1,
    ));
    let outfit = write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::PersonData, 8),
        -1,
    )
    .unwrap();
    assert!(outfit.signals.contains(&MemorySignal::OutfitRequest {
        entity: avatar,
        suit: 65535
    }));
}

#[test]
fn motive_overfill_uses_content_limits_and_checks_before_narrowing_indices() {
    let (mut state, content, [_, avatar, _]) = fixture();
    let target = address(avatar, EntityField::Motive, 5);
    write_memory(&mut state, &content, &target, 200).unwrap();
    assert_eq!(read_memory(&state, &content, &target), Ok(150));
    state
        .entities
        .get_mut(&avatar.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .motives
        .values[5] = 200;
    write_memory(&mut state, &content, &target, 190).unwrap();
    assert_eq!(read_memory(&state, &content, &target), Ok(190));
    write_memory(&mut state, &content, &target, -300).unwrap();
    assert_eq!(read_memory(&state, &content, &target), Ok(-100));
    let before = state.clone();
    assert!(write_memory(
        &mut state,
        &content,
        &address(avatar, EntityField::Motive, 256),
        10
    )
    .is_err());
    assert_eq!(state, before);
}

fn frame(owner: EntityRef, callee: EntityRef, stack: EntityRef) -> VmFrame {
    let mut context = FrameContext::for_entity(owner, 20);
    context.callee = callee;
    context.stack_object = stack.object_id;
    context.stack_object_ref = Some(stack);
    VmFrame {
        routine: RoutineKey {
            scope: RoutineScope::Global,
            id: 256,
        },
        instruction_pointer: 0,
        context,
        locals: Vec::new(),
        args: Vec::new(),
        special_result: SpecialResult::Normal,
        action_tree: true,
    }
}

#[test]
fn active_queue_users_frame_callees_and_stack_objects_have_distinct_source_roles() {
    let (mut state, content, [object, avatar, other]) = fixture();
    for reference in [object, other] {
        let value = state.entities.get_mut(&reference.object_id).unwrap();
        value.info.group = vec![object.object_id, other.object_id];
        value.info.base_object = object.object_id;
        value.info.multi_tile = true;
        value.queued_users.insert(avatar);
    }
    assert_eq!(
        read(&state, &content, object, EntityField::ObjectData, 62),
        1
    );
    let mut active = state.threads.remove(&avatar.object_id).unwrap();
    active.frames.push(frame(avatar, other, avatar));
    let query = address(object, EntityField::ObjectData, 79);
    write_memory_with_active(&mut state, &content, &query, 1, Some(&active)).unwrap();
    assert_eq!(read_memory(&state, &content, &query), Ok(0));
    active.frames[0] = frame(avatar, avatar, object);
    write_memory_with_active(&mut state, &content, &query, 1, Some(&active)).unwrap();
    assert_eq!(read_memory(&state, &content, &query), Ok(1)); // source recursive overload omits stack safety
    state.entities.get_mut(&object.object_id).unwrap().container = Some((avatar, 0));
    write_memory_with_active(&mut state, &content, &query, 1, Some(&active)).unwrap();
    assert_eq!(read_memory(&state, &content, &query), Ok(0));
    state
        .entities
        .get_mut(&avatar.object_id)
        .unwrap()
        .active_advertisements = Some(BTreeMap::from([((1, 5), 77)]));
    let ad = MemoryAddress::TreeAdvertisement { kind: 1, index: 5 };
    assert_eq!(
        read_memory_with_active(&state, &content, &ad, Some(&active)),
        Ok(77)
    );
    active.tree_advertisements.insert((1, 5), 22);
    assert_eq!(
        read_memory_with_active(&state, &content, &ad, Some(&active)),
        Ok(22)
    );
    assert_eq!(read_memory(&state, &content, &ad), Ok(0));
}

#[test]
fn room_memory_uses_base_room_area_zero_based_floors_and_selected_pool_state() {
    let (mut state, content, _) = fixture();
    let mut lot = LotModel::new(2, 2, 1).unwrap();
    lot.set_floor(TilePos::new(0, 0, 1), 65534).unwrap();
    state.world = WorldState::new(lot);
    let room = |room, index| MemoryAddress::Room { room, index };
    assert_eq!(read_memory(&state, &content, &room(i32::MIN, 0)), Ok(100));
    assert_eq!(read_memory(&state, &content, &room(i32::MIN, 3)), Ok(0));
    assert_eq!(read_memory(&state, &content, &room(1, 4)), Ok(1)); // source includes water in IsPool
    assert_eq!(read_memory(&state, &content, &room(2, 4)), Ok(0));
    assert_eq!(read_memory(&state, &content, &room(2, 2)), Ok(0));
    assert_eq!(read_memory(&state, &content, &room(2, 3)), Ok(1)); // base is one full water tile, not two half-cells
    assert_eq!(read_memory(&state, &content, &room(i32::MAX, 3)), Ok(1));
}

#[test]
fn stale_references_register_bounds_and_external_scopes_do_not_succeed_silently() {
    let (mut state, content, [object, avatar, _]) = fixture();
    let stale = EntityRef {
        generation: object.generation + 1,
        ..object
    };
    let before = state.clone();
    assert!(matches!(
        write_memory(
            &mut state,
            &content,
            &address(stale, EntityField::Attribute, 0),
            1
        ),
        Err(VmFault::StaleEntity(_))
    ));
    assert_eq!(state, before);
    write_memory(
        &mut state,
        &content,
        &address(object, EntityField::Temp, 19),
        -7,
    )
    .unwrap();
    assert_eq!(read(&state, &content, object, EntityField::Temp, 19), -7);
    assert!(read_memory(&state, &content, &address(object, EntityField::Temp, 20)).is_err());
    assert_eq!(read(&state, &content, object, EntityField::Slot, 65535), 0);
    assert!(read_memory(&state, &content, &address(avatar, EntityField::Slot, 3)).is_err());
    assert!(
        !write_memory(
            &mut state,
            &content,
            &address(object, EntityField::Definition, 65535),
            1
        )
        .unwrap()
        .written
    );
    state.mode = VmMode::Ts1;
    assert!(matches!(
        read_memory(
            &state,
            &content,
            &address(object, EntityField::TypeAttribute, 0)
        ),
        Err(VmFault::HostUnsupported(_))
    ));
    assert!(matches!(
        write_memory(
            &mut state,
            &content,
            &address(object, EntityField::TypeAttribute, 0),
            1
        ),
        Err(VmFault::HostUnsupported(_))
    ));
    let external = MemoryAddress::Neighborhood {
        scope: 32,
        object_id: object.object_id,
        data: 0,
        temp0: 0,
        temp1: 0,
    };
    assert!(matches!(
        read_memory(&state, &content, &external),
        Err(VmFault::HostUnsupported(_))
    ));
}
