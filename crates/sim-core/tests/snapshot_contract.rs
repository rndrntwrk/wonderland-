use bincode::Options;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sim_core::avatars::{
    social::RelationshipTarget as AvatarRelationshipTarget,
    state::TEMPLATE_PERSON,
    timeline::{AnimationMetadata, AnimationState},
    AvatarPlatform, AvatarState,
};
use sim_core::clock::SimClock;
use sim_core::effects::{
    EffectBook, EffectKind, EffectLimits, EffectPayload, EffectResolved, EffectValue,
};
use sim_core::ids::{EntityRef, IdAllocator, ObjectId, PersistentId};
use sim_core::rng::SimRng;
use sim_core::scheduler::Scheduler;
use sim_core::snapshot::{
    decode, encode, validate_state, SnapshotError, SnapshotExpectation, SnapshotLimits,
    MAX_SNAPSHOT_PAYLOAD_BYTES, SNAPSHOT_CHECKSUM_LEN, SNAPSHOT_HEADER_LEN, SNAPSHOT_MAGIC,
};
use sim_core::state::{
    AnimationKey, ContentSet, ContinuationKind, EntityState, HeadlineKind, HeadlineState,
    LifecyclePhase, ObjectDefinition, RuntimeContinuation, SimState, SimulationLimits, TuningSet,
    SIMULATION_SCHEMA,
};
use sim_core::vm::{
    EntityInfo, ExternalKind, ExternalRequest, FrameContext, HostRequest, HostResponse,
    PrimitiveContinuation, PrimitiveExit, RegisterTarget, RegisterWrite, RelationshipKey,
    RelationshipOwner, RelationshipTarget, RoutineKey, RoutineScope, RoutineStore,
    Ts1InventoryItem, VmInstruction, VmMode, VmPosition, VmResolution, VmRoutine, VmStop, VmThread,
    MAX_TS1_INVENTORY_ITEMS,
};
use sim_core::world::{
    LotModel, LotPosition, ObstacleMotion, RouteContinuation, RouteGoal, RouteRequest, WorldObject,
    WorldState,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const GUID: u32 = 0x1001;
const EXTERNAL_OPERAND: [u8; 8] = [0, 0, 5, 0, 0, 0, 0, 0];

fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .serialize(value)
        .unwrap()
}

fn key() -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(GUID),
        id: 4096,
    }
}

fn content() -> ContentSet {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(),
            VmRoutine::new(
                4096,
                2,
                4,
                vec![VmInstruction::new(25, 254, 255, EXTERNAL_OPERAND)],
            )
            .unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(GUID, 2);
    object.slot_count = 1;
    object.entry_points.insert(1, key());
    let mut avatar = ObjectDefinition::new(TEMPLATE_PERSON, 2);
    avatar.placement_rules.is_avatar = true;
    ContentSet::new(
        routines,
        vec![object, avatar],
        vec![(
            AnimationKey {
                owner: TEMPLATE_PERSON,
                scope: 0,
                id: 1,
            },
            AnimationMetadata {
                resource: "fixture.anim".into(),
                num_frames: 3,
                time_properties: Vec::new(),
            },
        )],
        TuningSet::default(),
    )
    .unwrap()
}

fn add_entity(state: &mut SimState, content: &ContentSet, avatar: bool) -> EntityRef {
    let reference = state.ids.allocate().unwrap();
    let guid = if avatar { TEMPLATE_PERSON } else { GUID };
    let definition = content.object(guid).unwrap();
    let position = VmPosition {
        x: 8 + (reference.object_id.0 - 1) * 16,
        y: 8,
        level: 1,
    };
    let mut avatar_state =
        avatar.then(|| AvatarState::new(reference, PersistentId(99), AvatarPlatform::Tso));
    if let Some(avatar) = &mut avatar_state {
        avatar.motives.limits = content.tuning().motive_limits;
    }
    let info = EntityInfo {
        reference,
        guid,
        master_guid: None,
        semiglobal: None,
        persistent_id: if avatar { 99 } else { 0 },
        is_avatar: avatar,
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
    let mut projection = WorldObject::new(
        reference,
        LotPosition::new(i32::from(position.x), i32::from(position.y), 1),
    );
    projection.footprint = definition.footprint.clone();
    projection.rules = definition.placement_rules.clone();
    projection.motion = if avatar {
        ObstacleMotion::StationaryAvatar
    } else {
        ObstacleMotion::Static
    };
    state.world.insert_object(projection).unwrap();
    state.entities.insert(
        reference.object_id,
        EntityState {
            info,
            attributes: definition.attributes.clone(),
            object_data: vec![0; 80],
            list: Vec::new(),
            dynamic_sprite_flags: vec![false; 128],
            type_attributes: BTreeMap::new(),
            tuning_overrides: BTreeMap::new(),
            active_advertisements: None,
            slots: vec![
                None;
                if avatar {
                    3
                } else {
                    usize::from(definition.slot_count)
                }
            ],
            container: None,
            initial_price: 0,
            lockout_started: 0,
            avatar: avatar_state,
            lifecycle: LifecyclePhase::Running,
            pending_entrypoints: VecDeque::new(),
            main_parameter: ObjectId(0),
            main_stack_object: ObjectId(0),
            queued_users: BTreeSet::new(),
            always_tick: avatar,
            disabled_flags: 0,
            broken: false,
            headline: None,
            revision: 0,
        },
    );
    state
        .threads
        .insert(reference.object_id, VmThread::new(reference, state.mode));
    state
        .scheduler
        .schedule(reference, state.completed_tick + 1)
        .unwrap();
    reference
}

fn fixture() -> (ContentSet, SimState, EntityRef) {
    let content = content();
    let mut state = SimState {
        schema: SIMULATION_SCHEMA,
        mode: VmMode::Tso,
        lot_id: 41,
        authority_epoch: 7,
        completed_tick: 0,
        content: content.descriptor().unwrap(),
        limits: SimulationLimits::default(),
        clock: SimClock::new(false, 0),
        rng: SimRng::new(123),
        ids: IdAllocator::new(),
        entities: BTreeMap::new(),
        threads: BTreeMap::new(),
        globals: vec![0; 38],
        relationships: sim_core::vm::RelationshipBook::default(),
        ts1_family_budget: None,
        ts1_inventory: sim_core::vm::Ts1InventoryBook::default(),
        scheduler: Scheduler::new(0),
        world: WorldState::new(LotModel::new(8, 8, 2).unwrap()),
        effects: EffectBook::new(41, EffectLimits::default()).unwrap(),
        continuations: BTreeMap::new(),
        next_continuation: 1,
        last_tick_digest: None,
    };
    let entity = add_entity(&mut state, &content, false);
    (content, state, entity)
}

fn ts1_fixture() -> (ContentSet, SimState, EntityRef) {
    let (content, mut state, entity) = fixture();
    state.mode = VmMode::Ts1;
    state.clock = SimClock::new(true, 0);
    for thread in state.threads.values_mut() {
        thread.mode = VmMode::Ts1;
    }
    (content, state, entity)
}

fn expectation(state: &SimState) -> SnapshotExpectation {
    SnapshotExpectation {
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        limits: SnapshotLimits::default(),
    }
}

fn repack(template: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = template[..SNAPSHOT_HEADER_LEN].to_vec();
    bytes[100..108].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload);
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&digest);
    bytes
}

fn rehash(bytes: &mut [u8]) {
    let end = bytes.len() - SNAPSHOT_CHECKSUM_LEN;
    let digest: [u8; 32] = Sha256::digest(&bytes[..end]).into();
    bytes[end..].copy_from_slice(&digest);
}

fn reject_state(content: &ContentSet, original: &SimState, change: impl FnOnce(&mut SimState)) {
    let before = canonical(original);
    let mut invalid = original.clone();
    change(&mut invalid);
    assert!(validate_state(&invalid, content).is_err());
    assert!(encode(&invalid, content).is_err());
    let forged = repack(&encode(original, content).unwrap(), &canonical(&invalid));
    assert!(decode(&forged, content, expectation(original)).is_err());
    assert_eq!(canonical(original), before);
}

fn arm_effect(state: &mut SimState, content: &ContentSet, owner: EntityRef) -> u64 {
    let context = FrameContext::for_entity(owner, GUID);
    let thread = state.threads.get_mut(&owner.object_id).unwrap();
    thread
        .push_entry(content.routines(), key(), context.clone(), vec![0; 4])
        .unwrap();
    let request = ExternalRequest {
        opcode: 25,
        kind: ExternalKind::TransferFunds,
        context,
        operand: EXTERNAL_OPERAND,
        parameters: vec![0; 4],
        temps: [0; 20],
        temp_xl: [0; 2],
        is_check: false,
        amount: Some(5),
    };
    let effect = state
        .effects
        .issue(
            owner,
            state.completed_tick,
            state.authority_epoch,
            EffectKind::Bytes,
            EffectPayload::Bytes(canonical(&request)),
        )
        .unwrap();
    let id = state.next_continuation;
    state.next_continuation += 1;
    thread.continuation = Some(PrimitiveContinuation {
        request_id: id,
        request: HostRequest::External(request),
        frame_depth: thread.frames.len(),
        instruction_pointer: 0,
        resolution: None,
    });
    thread.stop = VmStop::Waiting { request_id: id };
    state.continuations.insert(
        id,
        RuntimeContinuation {
            id,
            entity: owner,
            kind: ContinuationKind::Effect(effect.operation_id),
            resumes_vm: true,
        },
    );
    id
}

#[test]
fn fixed_header_and_post_tick_roundtrip_are_canonical() {
    let (content, mut state, owner) = fixture();
    for _ in 0..5 {
        state.clock.advance().unwrap();
    }
    state.completed_tick = 5;
    state.scheduler = Scheduler::new(5);
    state.scheduler.schedule(owner, 6).unwrap();
    state.last_tick_digest = Some([3; 32]);
    let bytes = encode(&state, &content).unwrap();
    assert_eq!(&bytes[..8], &SNAPSHOT_MAGIC);
    assert_eq!(SNAPSHOT_HEADER_LEN, 108);
    assert_eq!(u64::from_le_bytes(bytes[12..20].try_into().unwrap()), 41);
    assert_eq!(u64::from_le_bytes(bytes[20..28].try_into().unwrap()), 7);
    assert_eq!(u64::from_le_bytes(bytes[28..36].try_into().unwrap()), 5);
    assert_eq!(&bytes[36..68], &state.content.content_hash);
    assert_eq!(&bytes[68..100], &state.content.tuning_hash);
    assert_eq!(bytes, encode(&state, &content).unwrap());
    let restored = decode(&bytes, &content, expectation(&state)).unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.completed_tick.checked_add(1), Some(6));
    assert_eq!(encode(&restored, &content).unwrap(), bytes);
}

#[test]
fn checksum_covers_header_payload_and_digest_corruption() {
    let (content, state, _) = fixture();
    let original = encode(&state, &content).unwrap();
    for offset in [28, SNAPSHOT_HEADER_LEN + 20, original.len() - 1] {
        let mut corrupt = original.clone();
        corrupt[offset] ^= 1;
        assert_eq!(
            decode(&corrupt, &content, expectation(&state)),
            Err(SnapshotError::ChecksumMismatch)
        );
    }
}

#[test]
fn version_identity_and_content_expectations_are_explicit() {
    let (content, state, _) = fixture();
    let bytes = encode(&state, &content).unwrap();
    let mut wrong = expectation(&state);
    wrong.lot_id += 1;
    assert!(matches!(
        decode(&bytes, &content, wrong),
        Err(SnapshotError::LotMismatch { .. })
    ));
    let mut wrong = expectation(&state);
    wrong.authority_epoch += 1;
    assert!(matches!(
        decode(&bytes, &content, wrong),
        Err(SnapshotError::EpochMismatch { .. })
    ));
    for offset in [0, 8, 10] {
        let mut corrupt = bytes.clone();
        corrupt[offset] ^= 0x40;
        rehash(&mut corrupt);
        assert!(decode(&corrupt, &content, expectation(&state)).is_err());
    }
    for offset in [36, 68] {
        let mut corrupt = bytes.clone();
        corrupt[offset] ^= 1;
        rehash(&mut corrupt);
        assert!(decode(&corrupt, &content, expectation(&state)).is_err());
    }
}

#[test]
fn lengths_truncation_trailing_data_and_limits_are_rejected() {
    let (content, state, _) = fixture();
    let bytes = encode(&state, &content).unwrap();
    for len in [0, 7, SNAPSHOT_HEADER_LEN - 1, bytes.len() - 1] {
        assert!(decode(&bytes[..len], &content, expectation(&state)).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode(&trailing, &content, expectation(&state)).is_err());
    let mut wrong_len = bytes.clone();
    wrong_len[100..108].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        decode(&wrong_len, &content, expectation(&state)),
        Err(SnapshotError::PayloadTooLarge { .. })
    ));
    let mut limited = expectation(&state);
    limited.limits.max_payload_bytes = 1;
    assert!(matches!(
        decode(&bytes, &content, limited),
        Err(SnapshotError::PayloadTooLarge { .. })
    ));
    let mut invalid = expectation(&state);
    invalid.limits.max_payload_bytes = MAX_SNAPSHOT_PAYLOAD_BYTES + 1;
    assert!(matches!(
        decode(&[], &content, invalid),
        Err(SnapshotError::InvalidLimits(_))
    ));
    let mut payload = canonical(&state);
    payload.push(0);
    assert!(decode(&repack(&bytes, &payload), &content, expectation(&state)).is_err());
}

#[test]
fn header_and_canonical_payload_must_describe_the_same_state() {
    let (content, state, _) = fixture();
    let mut bytes = encode(&state, &content).unwrap();
    bytes[28..36].copy_from_slice(&9_u64.to_le_bytes());
    rehash(&mut bytes);
    assert!(matches!(
        decode(&bytes, &content, expectation(&state)),
        Err(SnapshotError::HeaderPayloadMismatch(_))
    ));
}

#[test]
fn duplicate_map_entries_are_rejected_after_valid_checksum() {
    let (content, state, _) = fixture();
    let mut payload = canonical(&state);
    let entity_bytes = canonical(&state.entities);
    let position = payload
        .windows(entity_bytes.len())
        .position(|bytes| bytes == entity_bytes)
        .unwrap();
    let entry = state.entities.iter().next().unwrap();
    // Fixed bincode maps and sequences both begin with their u64 length.
    // A duplicate map key would otherwise silently replace its first entry.
    let duplicate_entities = canonical(&vec![entry, entry]);
    payload.splice(position..position + entity_bytes.len(), duplicate_entities);
    let forged = repack(&encode(&state, &content).unwrap(), &payload);
    assert_eq!(
        decode(&forged, &content, expectation(&state)),
        Err(SnapshotError::NonCanonicalPayload)
    );
}

#[test]
fn entity_allocator_world_thread_and_avatar_graphs_are_checked() {
    let (content, mut state, owner) = fixture();
    let avatar = add_entity(&mut state, &content, true);
    reject_state(&content, &state, |s| {
        s.ids.release(owner).unwrap();
    });
    reject_state(&content, &state, |s| {
        s.threads.remove(&owner.object_id);
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .info
            .reference
            .generation += 1;
    });
    reject_state(&content, &state, |s| {
        s.world
            .move_object(
                owner,
                LotPosition::new(12, 8, 1),
                sim_core::world::Facing::NORTH,
            )
            .unwrap();
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&avatar.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .persistent_id = PersistentId(100);
    });
    reject_state(&content, &state, |s| {
        s.entities.get_mut(&owner.object_id).unwrap().info.dead = true;
    });
    reject_state(&content, &state, |s| {
        s.entities.get_mut(&owner.object_id).unwrap().lifecycle = LifecyclePhase::Exited;
    });
}

#[test]
fn semantic_bounds_apply_even_when_header_limits_are_large() {
    let (content, mut state, owner) = fixture();
    add_entity(&mut state, &content, false);
    reject_state(&content, &state, |s| {
        s.limits.max_entities = 1;
    });
    reject_state(&content, &state, |s| {
        s.globals.push(0);
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .attributes
            .pop();
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .object_data
            .push(0);
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .dynamic_sprite_flags
            .pop();
    });
    reject_state(&content, &state, |s| {
        s.entities.get_mut(&owner.object_id).unwrap().list =
            vec![0; s.limits.max_list_items as usize + 1];
    });
    let mut restricted = expectation(&state);
    restricted.limits.max_entities = 1;
    assert!(decode(&encode(&state, &content).unwrap(), &content, restricted).is_err());
}

#[test]
fn snapshots_only_capture_completed_scheduler_and_clock_boundaries() {
    let (content, state, owner) = fixture();
    reject_state(&content, &state, |s| {
        s.scheduler.begin_tick(1).unwrap();
    });
    reject_state(&content, &state, |s| {
        s.clock.advance().unwrap();
    });
    reject_state(&content, &state, |s| {
        s.last_tick_digest = Some([1; 32]);
    });
    reject_state(&content, &state, |s| {
        s.scheduler.cancel(owner);
        s.scheduler
            .schedule(
                EntityRef {
                    object_id: owner.object_id,
                    generation: owner.generation + 1,
                },
                1,
            )
            .unwrap();
    });
}

#[test]
fn cached_stale_frame_references_survive_without_rebinding() {
    let (content, mut state, owner) = fixture();
    let historical = state.ids.allocate().unwrap();
    state.ids.release(historical).unwrap();
    let mut context = FrameContext::for_entity(owner, GUID);
    context.caller = historical;
    context.callee = historical;
    context.stack_object = historical.object_id;
    context.stack_object_ref = Some(historical);
    state
        .threads
        .get_mut(&owner.object_id)
        .unwrap()
        .push_entry(content.routines(), key(), context, vec![0; 4])
        .unwrap();
    let restored = decode(
        &encode(&state, &content).unwrap(),
        &content,
        expectation(&state),
    )
    .unwrap();
    assert_eq!(
        restored.threads[&owner.object_id].frames[0]
            .context
            .stack_object_ref,
        Some(historical)
    );
    reject_state(&content, &state, |s| {
        s.threads.get_mut(&owner.object_id).unwrap().frames[0]
            .context
            .stack_object_ref
            .as_mut()
            .unwrap()
            .generation = 0;
    });
    reject_state(&content, &state, |s| {
        s.threads.get_mut(&owner.object_id).unwrap().frames[0].instruction_pointer = 200;
    });
}

#[test]
fn waiting_effects_and_runtime_continuations_form_a_bijection() {
    let (content, mut state, owner) = fixture();
    let id = arm_effect(&mut state, &content, owner);
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
    reject_state(&content, &state, |s| {
        s.continuations.clear();
    });
    reject_state(&content, &state, |s| {
        s.next_continuation = id;
    });
    reject_state(&content, &state, |s| {
        s.continuations.get_mut(&id).unwrap().resumes_vm = false;
    });
    reject_state(&content, &state, |s| {
        s.continuations.get_mut(&id).unwrap().entity.generation += 1;
    });
    reject_state(&content, &state, |s| {
        s.effects = EffectBook::new(41, EffectLimits::default()).unwrap();
    });
    reject_state(&content, &state, |s| {
        // The same operation identity does not authorize a different VM request.
        s.effects = EffectBook::new(41, EffectLimits::default()).unwrap();
        s.effects
            .issue(
                owner,
                0,
                7,
                EffectKind::Bytes,
                EffectPayload::Bytes(vec![9]),
            )
            .unwrap();
    });
    for change in 0..5 {
        reject_state(&content, &state, |s| {
            let HostRequest::External(request) = &mut s
                .threads
                .get_mut(&owner.object_id)
                .unwrap()
                .continuation
                .as_mut()
                .unwrap()
                .request
            else {
                unreachable!()
            };
            match change {
                0 => request.opcode = 0,
                1 => request.kind = ExternalKind::Sound,
                2 => request.operand[2] = 6,
                3 => request.parameters[0] = 99,
                _ => request.amount = Some(500),
            }
            let payload = canonical(request);
            // Both halves of the ownership link agree with one another, but
            // this request did not originate at the immutable BHAV instruction
            // with the still-suspended frame parameters.
            s.effects = EffectBook::new(41, EffectLimits::default()).unwrap();
            s.effects
                .issue(
                    owner,
                    0,
                    7,
                    EffectKind::Bytes,
                    EffectPayload::Bytes(payload),
                )
                .unwrap();
        });
    }
    let mut shared_registers = state.clone();
    let thread = shared_registers.threads.get_mut(&owner.object_id).unwrap();
    // Other entity scripts may update shared registers during the wait. The
    // request retains its issued values while frame-local parameters stay fixed.
    thread.temps[0] = 77;
    thread.temp_xl[1] = 90_000;
    let bytes = encode(&shared_registers, &content).unwrap();
    assert_eq!(
        decode(&bytes, &content, expectation(&shared_registers)).unwrap(),
        shared_registers
    );
}

#[test]
fn restored_ready_responses_validate_register_destinations() {
    let (content, mut state, owner) = fixture();
    let other = add_entity(&mut state, &content, false);
    let id = arm_effect(&mut state, &content, owner);
    let operation = match state.continuations.remove(&id).unwrap().kind {
        ContinuationKind::Effect(id) => id,
        _ => unreachable!(),
    };
    let resolution = VmResolution::complete(PrimitiveExit::GotoTrue);
    state
        .effects
        .resolve(
            EffectResolved {
                operation_id: operation,
                target: owner,
                apply_tick: 0,
                delivery_epoch: 7,
                committed_epoch: 7,
                value: EffectValue::Bytes(canonical(&resolution)),
            },
            0,
            7,
            Some(owner),
        )
        .unwrap();
    state
        .threads
        .get_mut(&owner.object_id)
        .unwrap()
        .resume(id, resolution)
        .unwrap();
    encode(&state, &content).unwrap();
    reject_state(&content, &state, |s| {
        // Retained ready completions still own globally unique request IDs.
        let mut duplicate = s.threads[&owner.object_id].clone();
        duplicate.owner = other;
        s.threads.insert(other.object_id, duplicate);
    });
    reject_state(&content, &state, |s| {
        s.threads
            .get_mut(&owner.object_id)
            .unwrap()
            .continuation
            .as_mut()
            .unwrap()
            .resolution
            .as_mut()
            .unwrap()
            .writes
            .push(RegisterWrite {
                target: RegisterTarget::TempXl,
                index: 2,
                value: 7,
            });
    });
    reject_state(&content, &state, |s| {
        s.threads
            .get_mut(&owner.object_id)
            .unwrap()
            .continuation
            .as_mut()
            .unwrap()
            .resolution
            .as_mut()
            .unwrap()
            .response = HostResponse::Pending { request_id: id + 1 };
    });
    for response in [
        HostResponse::Complete(PrimitiveExit::Continue),
        HostResponse::NextTick,
        HostResponse::AnimationEvent(7),
        HostResponse::CompleteWithWrites {
            exit: PrimitiveExit::GotoTrue,
            writes: Vec::new(),
        },
    ] {
        reject_state(&content, &state, |s| {
            s.threads
                .get_mut(&owner.object_id)
                .unwrap()
                .continuation
                .as_mut()
                .unwrap()
                .resolution
                .as_mut()
                .unwrap()
                .response = response;
        });
    }
}

#[test]
fn embedded_animation_metadata_must_match_expected_content() {
    let (content, mut state, _) = fixture();
    let owner = add_entity(&mut state, &content, true);
    let metadata = content.animation_metadata().next().unwrap().clone();
    state
        .entities
        .get_mut(&owner.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .animations
        .animations
        .push(AnimationState::new(metadata, false).unwrap());
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .animations
            .animations[0]
            .metadata
            .num_frames += 1;
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .animations
            .animations[0]
            .current_frame = f32::NAN;
    });
}

#[test]
fn standalone_routes_restore_with_live_idle_owner_and_future_schedule() {
    let (content, mut state, owner) = fixture();
    let id = state.next_continuation;
    state.next_continuation += 1;
    let route = RouteContinuation::new(
        RouteRequest::new(
            id,
            owner,
            LotPosition::new(8, 8, 1),
            vec![RouteGoal::point(LotPosition::new(24, 8, 1))],
        ),
        &state.world,
    )
    .unwrap();
    state.continuations.insert(
        id,
        RuntimeContinuation {
            id,
            entity: owner,
            kind: ContinuationKind::Route(route),
            resumes_vm: false,
        },
    );
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
    reject_state(&content, &state, |s| {
        s.scheduler.cancel(owner);
    });
    reject_state(&content, &state, |s| {
        s.continuations.get_mut(&id).unwrap().resumes_vm = true;
    });
}

#[test]
fn malicious_map_size_hint_is_rejected_before_entry_allocation() {
    let (content, state, _) = fixture();
    let mut payload = canonical(&state);
    let entity_bytes = canonical(&state.entities);
    let position = payload
        .windows(entity_bytes.len())
        .position(|bytes| bytes == entity_bytes)
        .unwrap();
    payload.truncate(position);
    payload.extend_from_slice(&u64::MAX.to_le_bytes());
    let forged = repack(&encode(&state, &content).unwrap(), &payload);
    assert!(matches!(decode(&forged, &content, expectation(&state)),
        Err(SnapshotError::Decode(message)) if message.contains("map limit")));
}

#[test]
fn malicious_nested_routes_are_rejected_before_unbounded_recursion() {
    let (content, mut state, owner) = fixture();
    let route = RouteContinuation::new(
        RouteRequest::new(
            1,
            owner,
            LotPosition::new(8, 8, 1),
            vec![RouteGoal::point(LotPosition::new(24, 8, 1))],
        ),
        &state.world,
    )
    .unwrap();
    let route_bytes = canonical(&route);
    state.continuations.insert(
        1,
        RuntimeContinuation {
            id: 1,
            entity: owner,
            kind: ContinuationKind::Route(route),
            resumes_vm: false,
        },
    );
    state.next_continuation = 2;
    let encoded = encode(&state, &content).unwrap();
    let mut payload = canonical(&state);
    let position = payload
        .windows(route_bytes.len())
        .position(|bytes| bytes == route_bytes)
        .unwrap();

    // RouteContinuation ends with its nested-child vector. A chain of 2,001
    // otherwise decodable structs is small on the wire but unsafe for an
    // unguarded recursive derived decoder before semantic depth validation.
    assert_eq!(&route_bytes[route_bytes.len() - 8..], &0_u64.to_le_bytes());
    let prefix = &route_bytes[..route_bytes.len() - 8];
    let mut nested = Vec::new();
    for _ in 0..2_000 {
        nested.extend_from_slice(prefix);
        nested.extend_from_slice(&1_u64.to_le_bytes());
    }
    nested.extend_from_slice(&route_bytes);
    payload.splice(position..position + route_bytes.len(), nested);
    let forged = repack(&encoded, &payload);
    assert!(matches!(decode(&forged, &content, expectation(&state)),
        Err(SnapshotError::Decode(message)) if message.contains("nesting limit")));
}

#[test]
fn containment_is_reciprocal_acyclic_and_preserves_world_projection() {
    let (content, mut state, parent) = fixture();
    let child = add_entity(&mut state, &content, false);
    let parent_position = state.entities[&parent.object_id].info.position;
    state.entities.get_mut(&parent.object_id).unwrap().slots[0] = Some(child);
    let child_entity = state.entities.get_mut(&child.object_id).unwrap();
    child_entity.container = Some((parent, 0));
    child_entity.info.position = parent_position;
    let mut projection = state.world.object(child).unwrap().clone();
    projection.position = state.world.object(parent).unwrap().position;
    projection.rules.zero_extent = true;
    state.world.replace_object(projection).unwrap();
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
    reject_state(&content, &state, |s| {
        s.entities.get_mut(&parent.object_id).unwrap().slots[0] = None;
    });
    reject_state(&content, &state, |s| {
        s.entities.get_mut(&parent.object_id).unwrap().container = Some((child, 0));
        s.entities.get_mut(&child.object_id).unwrap().slots[0] = Some(parent);
        let mut projection = s.world.object(parent).unwrap().clone();
        projection.rules.zero_extent = true;
        s.world.replace_object(projection).unwrap();
    });
}

#[test]
fn source_out_of_world_sentinel_is_valid_snapshot_state() {
    let (content, mut state, owner) = fixture();
    state
        .entities
        .get_mut(&owner.object_id)
        .unwrap()
        .info
        .position = VmPosition {
        x: i16::MIN,
        y: i16::MIN,
        level: 1,
    };
    state
        .world
        .move_object(
            owner,
            LotPosition::OUT_OF_WORLD,
            sim_core::world::Facing::NORTH,
        )
        .unwrap();
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
}

#[test]
fn relationship_bookkeeping_supersets_restore_with_exact_avatar_projection() {
    let (content, mut state, target) = fixture();
    let owner = add_entity(&mut state, &content, true);
    let local = RelationshipKey {
        owner: RelationshipOwner::Entity(owner),
        target: RelationshipTarget::Local(target),
    };
    state.relationships.write(local, vec![10, 20]).unwrap();
    state.relationships.mark(local, false).unwrap();
    // Source ClearRelationships and early dirtiness marking can leave both
    // bookkeeping indexes populated without corresponding matrix entries.
    state
        .relationships
        .mark(
            RelationshipKey {
                owner: RelationshipOwner::Entity(owner),
                target: RelationshipTarget::Persistent(42),
            },
            true,
        )
        .unwrap();
    state
        .relationships
        .local_reverse
        .entry(owner)
        .or_default()
        .insert(target);
    let avatar = state
        .entities
        .get_mut(&owner.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar
        .relationships
        .values
        .insert(AvatarRelationshipTarget::Local(target), vec![10, 20]);
    avatar
        .relationships
        .changed_persistent
        .insert(PersistentId(42));
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );

    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .relationships
            .values
            .get_mut(&AvatarRelationshipTarget::Local(target))
            .unwrap()[1] = 21;
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .relationships
            .changed_persistent
            .clear();
    });
    reject_state(&content, &state, |s| {
        s.relationships.matrices.clear();
    });
}

#[test]
fn relationships_reject_stale_generations_invalid_identity_and_oversized_rows() {
    let (content, mut state, owner) = fixture();
    let target = add_entity(&mut state, &content, false);
    let local = RelationshipKey {
        owner: RelationshipOwner::Entity(owner),
        target: RelationshipTarget::Local(target),
    };
    state.relationships.write(local, vec![10, 20]).unwrap();
    state.relationships.mark(local, false).unwrap();
    encode(&state, &content).unwrap();

    reject_state(&content, &state, |s| {
        let values = s.relationships.matrices.remove(&local).unwrap();
        let mut stale = target;
        stale.generation += 1;
        s.relationships.matrices.insert(
            RelationshipKey {
                target: RelationshipTarget::Local(stale),
                ..local
            },
            values,
        );
    });
    reject_state(&content, &state, |s| {
        s.relationships
            .matrices
            .get_mut(&local)
            .unwrap()
            .resize(257, 0);
    });
    reject_state(&content, &state, |s| {
        s.relationships.changed_persistent.insert((owner, 0));
    });
    reject_state(&content, &state, |s| {
        let mut stale = owner;
        stale.generation += 1;
        s.relationships
            .local_reverse
            .get_mut(&target)
            .unwrap()
            .insert(stale);
    });
}

#[test]
fn source_headline_duration_bounds_preserve_historical_icon_and_wrapping_counter() {
    let (content, mut state, owner) = fixture();
    let mut historical = owner;
    historical.generation += 1;
    state.entities.get_mut(&owner.object_id).unwrap().headline = Some(HeadlineState {
        duration: 491_505,
        anim: i32::MAX,
        kind: HeadlineKind::Balloon {
            icon: Some(historical),
            index: -1,
            group: 0,
            headline_type: 0,
            flags: 8,
        },
    });
    for duration in [-491_520, -1, 0, 491_505] {
        state
            .entities
            .get_mut(&owner.object_id)
            .unwrap()
            .headline
            .as_mut()
            .unwrap()
            .duration = duration;
        let encoded = encode(&state, &content).unwrap();
        assert_eq!(
            decode(&encoded, &content, expectation(&state)).unwrap(),
            state
        );
    }
    for duration in [-491_521, 491_506] {
        reject_state(&content, &state, |s| {
            s.entities
                .get_mut(&owner.object_id)
                .unwrap()
                .headline
                .as_mut()
                .unwrap()
                .duration = duration;
        });
    }
    reject_state(&content, &state, |s| {
        let headline = s
            .entities
            .get_mut(&owner.object_id)
            .unwrap()
            .headline
            .as_mut()
            .unwrap();
        if let HeadlineKind::Balloon {
            icon: Some(icon), ..
        } = &mut headline.kind
        {
            icon.generation = 0;
        }
    });
}

#[test]
fn multitile_groups_restore_once_per_base_and_reject_divergent_membership() {
    let (content, mut state, base) = fixture();
    let member = add_entity(&mut state, &content, false);
    let group = vec![base.object_id, member.object_id];
    for reference in [base, member] {
        let info = &mut state.entities.get_mut(&reference.object_id).unwrap().info;
        info.group = group.clone();
        info.multi_tile = true;
        info.base_object = base.object_id;
        info.persistent_id = 55;
        let mut projection = state.world.object(reference).unwrap().clone();
        projection.multitile_group = Some(base);
        state.world.replace_object(projection).unwrap();
    }
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );

    reject_state(&content, &state, |s| {
        s.entities.get_mut(&member.object_id).unwrap().info.group = vec![member.object_id];
    });
    reject_state(&content, &state, |s| {
        for reference in [base, member] {
            s.entities
                .get_mut(&reference.object_id)
                .unwrap()
                .info
                .group
                .push(member.object_id);
        }
    });
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&member.object_id)
            .unwrap()
            .info
            .base_object = member.object_id;
    });
    reject_state(&content, &state, |s| {
        let mut projection = s.world.object(member).unwrap().clone();
        projection.multitile_group = None;
        s.world.replace_object(projection).unwrap();
    });
}

#[test]
fn source_lazy_attribute_growth_survives_snapshot_validation() {
    let (content, mut state, owner) = fixture();
    for count in [4, 4096] {
        let attributes = &mut state.entities.get_mut(&owner.object_id).unwrap().attributes;
        attributes.resize(count, 0);
        attributes[3] = 42;
        let encoded = encode(&state, &content).unwrap();
        assert_eq!(
            decode(&encoded, &content, expectation(&state)).unwrap(),
            state
        );
    }
    reject_state(&content, &state, |s| {
        s.entities
            .get_mut(&owner.object_id)
            .unwrap()
            .attributes
            .push(0);
    });
}

#[test]
fn ts1_budget_and_historical_inventory_preserve_source_values() {
    let (content, mut state, _) = ts1_fixture();
    let token = Ts1InventoryItem {
        token_type: i32::MIN,
        guid: 0,
        count: u16::MAX,
    };
    state
        .ts1_inventory
        .inventories
        .insert(-32, vec![token, token]);
    state.ts1_inventory.inventories.insert(0, Vec::new());
    state
        .ts1_inventory
        .inventories
        .insert(i16::MAX, vec![token]);
    for budget in [None, Some(i32::MIN), Some(-1), Some(i32::MAX)] {
        state.ts1_family_budget = budget;
        let encoded = encode(&state, &content).unwrap();
        assert_eq!(
            decode(&encoded, &content, expectation(&state)).unwrap(),
            state
        );
    }
}

#[test]
fn tso_snapshots_reject_ts1_neighborhood_projections() {
    let (content, state, _) = fixture();
    reject_state(&content, &state, |s| {
        s.ts1_family_budget = Some(0);
    });
    reject_state(&content, &state, |s| {
        s.ts1_inventory.inventories.insert(0, Vec::new());
    });
    reject_state(&content, &state, |s| {
        s.ts1_inventory.inventories.insert(
            99,
            vec![Ts1InventoryItem {
                token_type: 1,
                guid: 10,
                count: 1,
            }],
        );
    });
}

#[test]
fn ts1_inventory_enforces_per_neighbor_and_total_item_caps() {
    let (content, mut state, _) = ts1_fixture();
    let token = Ts1InventoryItem {
        token_type: 1,
        guid: 10,
        count: 1,
    };
    state
        .ts1_inventory
        .inventories
        .insert(0, vec![token; MAX_TS1_INVENTORY_ITEMS]);
    let encoded = encode(&state, &content).unwrap();
    assert_eq!(
        decode(&encoded, &content, expectation(&state)).unwrap(),
        state
    );
    reject_state(&content, &state, |s| {
        s.ts1_inventory.inventories.get_mut(&0).unwrap().push(token);
    });
    reject_state(&content, &state, |s| {
        s.ts1_inventory.inventories.insert(1, vec![token]);
    });
}
