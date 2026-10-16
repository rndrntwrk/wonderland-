//! Replicated metadata only; these tests do not admit actors or run join effects.
use bincode::Options;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use sha2::{Digest, Sha256};
#[path = "support/accepted_commands_v2.rs"]
mod accepted_commands_v2;
use accepted_commands_v2::LegacyAcceptedCommand;
use sim_core::{
    ids::PersistentId,
    lot_permissions::{LotPermissionState, ReplicatedLotPermissions, MAX_PERMISSION_ENTRIES},
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeError, RuntimeRole, SimRuntime, SpawnSpec},
    snapshot::{self, SnapshotError, SnapshotExpectation, SnapshotLimits, SNAPSHOT_HEADER_LEN},
    state::{ContentSet, ObjectDefinition, SimState, TuningSet},
    vm::{RoutineStore, VmMode},
    world::{Facing, LotModel, LotPosition},
};

fn runtime(mode: VmMode, role: RuntimeRole) -> SimRuntime {
    SimRuntime::new(
        ContentSet::new(
            RoutineStore::new(),
            vec![ObjectDefinition::new(100, 1)],
            vec![],
            TuningSet::default(),
        )
        .unwrap(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(mode, 41, 7, 123),
        role,
    )
    .unwrap()
}

fn lot() -> LotPermissionState {
    LotPermissionState {
        owner_id: PersistentId(0),
        property_category: 11,
        size: -1,
        roommates: [PersistentId(0), PersistentId(23)].into(),
        build_roommates: [PersistentId(42)].into(),
        object_limit: 17,
        limit_exceeded: true,
        donate_limit: 2345,
    }
}

fn canonical(value: &impl Serialize) -> Vec<u8> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .serialize(value)
        .unwrap()
}

fn initialize(lot: LotPermissionState) -> AcceptedCommand {
    AcceptedCommand::InitializeLotPermissions { lot }
}

fn spawn() -> AcceptedCommand {
    AcceptedCommand::Spawn(SpawnSpec {
        guid: 100,
        position: LotPosition::new(8, 8, 1),
        facing: Facing::NORTH,
        persistent_id: PersistentId(91),
        avatar: false,
    })
}

fn reject_commands(runtime: &mut SimRuntime, commands: Vec<AcceptedCommand>) {
    let before = runtime.snapshot().unwrap();
    let tick = runtime.next_tick(commands).unwrap();
    assert!(matches!(
        runtime.step(&tick),
        Err(RuntimeError::InvalidCommand(_))
    ));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn new_runtimes_start_current_schema_with_uninitialized_permissions() {
    for mode in [VmMode::Tso, VmMode::Ts1] {
        let runtime = runtime(mode, RuntimeRole::Authority);
        assert_eq!(runtime.state().schema, sim_core::state::SIMULATION_SCHEMA);
        assert_eq!(
            runtime.state().lot_permissions,
            ReplicatedLotPermissions::Uninitialized
        );
        assert!(canonical(runtime.state()).ends_with(&canonical(&(
            0_u32, 0_u32, 0_u64, 0_u64, 0_u64, true, 0_u64
        ))));
    }
}

#[test]
fn trusted_initialization_preserves_explicit_metadata_without_reconciliation() {
    let mut runtime = runtime(VmMode::Tso, RuntimeRole::Authority);
    let tick = runtime.next_tick(vec![initialize(lot())]).unwrap();
    let result = runtime.step(&tick).unwrap();
    assert!(result.events.is_empty());
    assert!(result.effects.is_empty());
    assert_eq!(
        runtime.state().lot_permissions,
        ReplicatedLotPermissions::Initialized(lot())
    );
    assert!(canonical(runtime.state()).ends_with(&canonical(&(
        1_u32,
        lot(),
        0_u32,
        0_u64,
        0_u64,
        0_u64,
        true,
        0_u64
    ))));
    assert!(runtime.state().entities.is_empty());
    assert!(runtime.state().interaction_access.is_empty());
}

#[test]
fn initialization_on_existing_lot_changes_only_metadata_and_accepted_digest() {
    let mut initialized = runtime(VmMode::Tso, RuntimeRole::Authority);
    let first = initialized.next_tick(vec![spawn()]).unwrap();
    initialized.step(&first).unwrap();
    let mut state = initialized.state().clone();
    let mut object = state.world.objects().values().next().unwrap().clone();
    object.owner = Some(PersistentId(23));
    state.world.replace_object(object).unwrap();
    initialized = SimRuntime::from_state(
        state.clone(),
        initialized.content().clone(),
        RuntimeRole::Authority,
    )
    .unwrap();
    let mut control =
        SimRuntime::from_state(state, initialized.content().clone(), RuntimeRole::Replica).unwrap();
    let init = initialized.next_tick(vec![initialize(lot())]).unwrap();
    let empty = control.next_tick(vec![]).unwrap();
    let actual = initialized.step(&init).unwrap();
    let expected = control.step(&empty).unwrap();
    assert_eq!(actual.events, expected.events);
    assert_eq!(actual.effects, expected.effects);
    assert_eq!(actual.instructions, expected.instructions);
    let mut expected = control.state().clone();
    expected.lot_permissions = ReplicatedLotPermissions::Initialized(lot());
    expected.last_tick_digest = initialized.state().last_tick_digest;
    assert_eq!(initialized.state(), &expected);
}

#[test]
fn initialized_metadata_is_replicated_restored_and_exact_replay_is_suppressed() {
    let mut authority = runtime(VmMode::Tso, RuntimeRole::Authority);
    let mut replica = runtime(VmMode::Tso, RuntimeRole::Replica);
    let tick = authority.next_tick(vec![initialize(lot())]).unwrap();
    let a = authority.step(&tick).unwrap();
    let b = replica.step(&tick).unwrap();
    assert_eq!(a.state_hash, b.state_hash);
    assert_eq!(a.events, b.events);
    assert_eq!(authority.snapshot().unwrap(), replica.snapshot().unwrap());
    let saved = authority.snapshot().unwrap();
    let mut recovered = runtime(VmMode::Tso, RuntimeRole::Authority);
    recovered.restore(&saved).unwrap();
    for peer in [&mut authority, &mut replica, &mut recovered] {
        let before = peer.snapshot().unwrap();
        let result = peer.step(&tick).unwrap();
        assert!(result.duplicate);
        assert_eq!(result.instructions, 0);
        assert!(result.events.is_empty());
        assert!(result.effects.is_empty());
        assert_eq!(peer.snapshot().unwrap(), before);
        let mut conflict = tick.clone();
        conflict.commands.clear();
        assert_eq!(peer.step(&conflict), Err(RuntimeError::ConflictingTick));
        assert_eq!(peer.snapshot().unwrap(), before);
    }
    for _ in 0..3 {
        let next = authority.next_tick(vec![]).unwrap();
        let a = authority.step(&next).unwrap();
        assert_eq!(a.state_hash, replica.step(&next).unwrap().state_hash);
        assert_eq!(a.state_hash, recovered.step(&next).unwrap().state_hash);
    }
}

#[test]
fn later_equal_or_changed_initialization_and_double_initialization_are_atomic_errors() {
    let mut runtime = runtime(VmMode::Tso, RuntimeRole::Authority);
    reject_commands(&mut runtime, vec![initialize(lot()), initialize(lot())]);
    let tick = runtime.next_tick(vec![initialize(lot())]).unwrap();
    runtime.step(&tick).unwrap();
    reject_commands(&mut runtime, vec![initialize(lot())]);
    let mut changed = lot();
    changed.owner_id = PersistentId(123);
    reject_commands(&mut runtime, vec![initialize(changed)]);
}

#[test]
fn initializer_refuses_invalid_metadata_and_ts1_without_partial_mutation() {
    let mut tso = runtime(VmMode::Tso, RuntimeRole::Authority);
    for case in 0..5 {
        let mut invalid = lot();
        match case {
            0 => {
                invalid.property_category = 0;
                invalid.size = 14;
            }
            1 => invalid.object_limit = -1,
            2 => invalid.donate_limit = -1,
            3 => {
                invalid.roommates = (0..=MAX_PERMISSION_ENTRIES as u32)
                    .map(PersistentId)
                    .collect()
            }
            4 => {
                invalid.build_roommates = (0..=MAX_PERMISSION_ENTRIES as u32)
                    .map(PersistentId)
                    .collect()
            }
            _ => unreachable!(),
        }
        reject_commands(&mut tso, vec![spawn(), initialize(invalid)]);
    }
    let mut ts1 = runtime(VmMode::Ts1, RuntimeRole::Authority);
    reject_commands(&mut ts1, vec![spawn(), initialize(lot())]);
}

#[test]
fn initialized_snapshot_validation_rejects_invalid_metadata_and_ts1() {
    for case in 0..4 {
        let mode = if case == 0 { VmMode::Ts1 } else { VmMode::Tso };
        let mut runtime = runtime(mode, RuntimeRole::Authority);
        let mut state = runtime.state().clone();
        let mut invalid = lot();
        match case {
            0 => {}
            1 => invalid.object_limit = -1,
            2 => {
                invalid.property_category = 0;
                invalid.size = 14;
            }
            3 => {
                invalid.build_roommates = (0..=MAX_PERMISSION_ENTRIES as u32)
                    .map(PersistentId)
                    .collect()
            }
            _ => unreachable!(),
        }
        state.lot_permissions = ReplicatedLotPermissions::Initialized(invalid);
        assert!(matches!(
            snapshot::encode(&state, runtime.content()),
            Err(SnapshotError::InvalidState {
                component: "lot_permissions",
                ..
            })
        ));
        let before = runtime.snapshot().unwrap();
        let malformed = frame(&before, 4, &canonical(&state));
        assert!(runtime.restore(&malformed).is_err());
        assert_eq!(runtime.snapshot().unwrap(), before);
        assert!(
            SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority)
                .is_err()
        );
    }
}

#[test]
fn valid_source_sentinels_caches_and_membership_bounds_are_preserved() {
    for category in [0, 11, 255] {
        let mut runtime = runtime(VmMode::Tso, RuntimeRole::Authority);
        let mut lot = lot();
        lot.property_category = category;
        if category == 0 {
            lot.size = (2 << 16) | (3 << 8) | 10;
        }
        lot.roommates = (0..MAX_PERMISSION_ENTRIES as u32)
            .map(PersistentId)
            .collect();
        lot.build_roommates = (1..=MAX_PERMISSION_ENTRIES as u32)
            .map(PersistentId)
            .collect();
        let tick = runtime.next_tick(vec![initialize(lot.clone())]).unwrap();
        runtime.step(&tick).unwrap();
        assert_eq!(
            runtime.state().lot_permissions,
            ReplicatedLotPermissions::Initialized(lot)
        );
        let saved = runtime.snapshot().unwrap();
        runtime.restore(&saved).unwrap();
        assert_eq!(runtime.snapshot().unwrap(), saved);
    }
}

// Freeze the old field order independently of the current SimState suffix.
// These records include real runtime entities and a real accepted-tick digest.
struct LegacyState<'a> {
    state: &'a SimState,
    schema: u16,
}
impl Serialize for LegacyState<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record =
            serializer.serialize_struct("SimState", if self.schema == 1 { 22 } else { 24 })?;
        record.serialize_field("schema", &self.schema)?;
        macro_rules! fields {
            ($($field:ident),* $(,)?) => { $(record.serialize_field(stringify!($field), &self.state.$field)?;)* };
        }
        fields!(
            mode,
            lot_id,
            authority_epoch,
            completed_tick,
            content,
            limits,
            clock,
            rng,
            ids,
            entities,
            threads,
            globals,
            relationships,
            ts1_family_budget,
            ts1_inventory,
            scheduler,
            world,
            effects,
            continuations,
            next_continuation,
            last_tick_digest
        );
        if self.schema == 2 {
            fields!(interaction_queues, interaction_access);
        }
        record.end()
    }
}

fn frame(template: &[u8], schema: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = template[..SNAPSHOT_HEADER_LEN].to_vec();
    bytes[10..12].copy_from_slice(&schema.to_le_bytes());
    bytes[100..108].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload);
    let checksum = Sha256::digest(&bytes);
    bytes.extend_from_slice(&checksum);
    bytes
}

fn legacy_frame(runtime: &SimRuntime, schema: u16) -> Vec<u8> {
    frame(
        &runtime.snapshot().unwrap(),
        schema,
        &canonical(&LegacyState {
            state: runtime.state(),
            schema,
        }),
    )
}

#[test]
fn schemas_one_and_two_migrate_without_inventing_metadata_or_losing_replay_receipts() {
    for schema in [1, 2] {
        for mode in [VmMode::Tso, VmMode::Ts1] {
            let mut original = runtime(mode, RuntimeRole::Authority);
            let last_tick = original.next_tick(vec![spawn()]).unwrap();
            original.step(&last_tick).unwrap();
            if schema == 2 {
                let entity = original
                    .state()
                    .entities
                    .values()
                    .next()
                    .unwrap()
                    .info
                    .reference;
                let mut state = original.state().clone();
                state.interaction_access.insert(
                    entity,
                    sim_core::runtime::interaction_adapter::InteractionAccess {
                        principal: sim_core::interactions::PrincipalKey(72),
                        allow_hidden: true,
                    },
                );
                original = SimRuntime::from_state(
                    state,
                    original.content().clone(),
                    RuntimeRole::Authority,
                )
                .unwrap();
            }
            // Legacy snapshots carry no proof that EOD sessions are clear.
            let mut state = original.state().clone();
            state.owned_departures.eod_known_clear = false;
            original =
                SimRuntime::from_state(state, original.content().clone(), RuntimeRole::Authority)
                    .unwrap();
            let legacy = legacy_frame(&original, schema);
            let mut recovered = runtime(mode, RuntimeRole::Replica);
            recovered.restore(&legacy).unwrap();
            assert_eq!(recovered.state(), original.state());
            assert_eq!(
                recovered.state().lot_permissions,
                ReplicatedLotPermissions::Uninitialized
            );
            assert!(recovered.step(&last_tick).unwrap().duplicate);
            assert_eq!(recovered.state(), original.state());
            let resaved = recovered.snapshot().unwrap();
            assert_eq!(
                &resaved[10..12],
                &sim_core::state::SIMULATION_SCHEMA.to_le_bytes()
            );
            recovered.restore(&resaved).unwrap();
            assert_eq!(recovered.state(), original.state());
            if mode == VmMode::Tso {
                let initialize = original.next_tick(vec![initialize(lot())]).unwrap();
                assert_eq!(
                    original.step(&initialize).unwrap().state_hash,
                    recovered.step(&initialize).unwrap().state_hash
                );
            }
        }
    }
}

#[test]
fn migration_respects_original_payload_limit_and_adds_only_its_known_suffix() {
    let original = runtime(VmMode::Tso, RuntimeRole::Authority);
    for schema in [1, 2] {
        let bytes = legacy_frame(&original, schema);
        let payload_len = (bytes.len() - SNAPSHOT_HEADER_LEN - 32) as u64;
        let mut expectation = SnapshotExpectation::new(41, 7);
        expectation.limits = SnapshotLimits {
            max_payload_bytes: payload_len,
            ..SnapshotLimits::default()
        };
        assert_eq!(
            snapshot::decode(&bytes, original.content(), expectation).unwrap(),
            {
                let mut expected = original.state().clone();
                expected.owned_departures.eod_known_clear = false;
                expected
            }
        );
        expectation.limits.max_payload_bytes -= 1;
        assert!(matches!(
            snapshot::decode(&bytes, original.content(), expectation),
            Err(SnapshotError::PayloadTooLarge { .. })
        ));
    }
}

#[test]
fn old_schemas_reject_extra_fields_truncation_and_header_payload_schema_mismatch() {
    let mut runtime = runtime(VmMode::Tso, RuntimeRole::Authority);
    let before = runtime.snapshot().unwrap();
    for schema in [1, 2] {
        let payload = canonical(&LegacyState {
            state: runtime.state(),
            schema,
        });
        for case in 0..4 {
            let mut malformed = payload.clone();
            match case {
                0 => malformed.extend_from_slice(&[0; 4]),
                1 => {
                    malformed.pop();
                }
                2 => malformed[..2]
                    .copy_from_slice(&sim_core::state::SIMULATION_SCHEMA.to_le_bytes()),
                3 => malformed = vec![schema as u8],
                _ => unreachable!(),
            }
            assert!(
                runtime
                    .restore(&frame(&before, schema, &malformed))
                    .is_err(),
                "schema {schema}, case {case}"
            );
            assert_eq!(runtime.snapshot().unwrap(), before);
        }
    }
    for schema in [0, sim_core::state::SIMULATION_SCHEMA + 1, u16::MAX] {
        let bytes = frame(&before, schema, &canonical(runtime.state()));
        assert_eq!(
            snapshot::decode(&bytes, runtime.content(), SnapshotExpectation::new(41, 7)),
            Err(SnapshotError::UnsupportedSchema(schema))
        );
    }
}

#[test]
fn initialized_payload_rejects_unknown_tags_huge_hints_and_noncanonical_membership_sets() {
    let mut runtime = runtime(VmMode::Tso, RuntimeRole::Authority);
    let before = runtime.snapshot().unwrap();
    let mut prefix = canonical(runtime.state());
    assert_eq!(&prefix[prefix.len() - 4..], &[0; 4]);
    prefix.truncate(prefix.len() - 8 - 16 - 17);
    prefix[..2].copy_from_slice(&4u16.to_le_bytes());
    let valid = canonical(&ReplicatedLotPermissions::Initialized(lot()));
    // enum(u32), owner(u32), category(u8), size(i32), roommate count(u64).
    let room_count = 13;
    let room_ids = room_count + 8;
    for case in 0..7 {
        let mut suffix = valid.clone();
        match case {
            0 => suffix[..4].copy_from_slice(&2_u32.to_le_bytes()),
            1 => suffix[room_count..room_ids].copy_from_slice(&u64::MAX.to_le_bytes()),
            2 => suffix[room_ids + 4..room_ids + 8].copy_from_slice(&0_u32.to_le_bytes()),
            3 => {
                suffix[room_ids..room_ids + 4].copy_from_slice(&23_u32.to_le_bytes());
                suffix[room_ids + 4..room_ids + 8].copy_from_slice(&0_u32.to_le_bytes());
            }
            4 => {
                suffix.pop();
            }
            5 => suffix.extend_from_slice(&[0]),
            6 => suffix.truncate(3),
            _ => unreachable!(),
        }
        let mut payload = prefix.clone();
        payload.extend_from_slice(&suffix);
        payload.extend_from_slice(&[0; 4]);
        let error = snapshot::decode(
            &frame(&before, 4, &payload),
            runtime.content(),
            SnapshotExpectation::new(41, 7),
        )
        .unwrap_err();
        if matches!(case, 2 | 3) {
            assert_eq!(error, SnapshotError::NonCanonicalPayload);
        }
        assert!(runtime.restore(&frame(&before, 4, &payload)).is_err());
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}

#[test]
fn existing_accepted_command_tags_are_preserved_and_initializer_is_appended() {
    for expected in 0_u32..=31 {
        // Zero-valued payload fields are decodable fixtures, not admissible commands.
        // This checks every discriminant without relying on unrelated constructors.
        let mut bytes = expected.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0; 4096]);
        let command: AcceptedCommand = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .allow_trailing_bytes()
            .deserialize(&bytes)
            .unwrap();
        let actual = match &command {
            AcceptedCommand::Spawn(_) => 0,
            AcceptedCommand::Delete { .. } => 1,
            AcceptedCommand::Interrupt { .. } => 2,
            AcceptedCommand::StartBehavior { .. } => 3,
            AcceptedCommand::WriteMemory { .. } => 4,
            AcceptedCommand::SetInteractionProjection { .. } => 5,
            AcceptedCommand::SetEveryFrame { .. } => 6,
            AcceptedCommand::SetObjectStatus { .. } => 7,
            AcceptedCommand::BeginRoute { .. } => 8,
            AcceptedCommand::RouteCallback { .. } => 9,
            AcceptedCommand::DefineSlot(_) => 10,
            AcceptedCommand::ReserveSlot(_) => 11,
            AcceptedCommand::OccupySlot(_) => 12,
            AcceptedCommand::ReleaseSlot(_) => 13,
            AcceptedCommand::EffectResolved(_) => 14,
            AcceptedCommand::Disconnect { .. } => 15,
            AcceptedCommand::Reconnect { .. } => 16,
            AcceptedCommand::Reset { .. } => 17,
            AcceptedCommand::SetTs1FamilyBudget(_) => 18,
            AcceptedCommand::SetTs1Inventory { .. } => 19,
            AcceptedCommand::SetInteractionAuthority { .. } => 20,
            AcceptedCommand::QueueInteraction(_) => 21,
            AcceptedCommand::CancelInteraction(_) => 22,
            AcceptedCommand::BeginBuild { .. } => 23,
            AcceptedCommand::CompleteBuild(_) => 24,
            AcceptedCommand::ExecuteNamedEntryPoint { .. } => 25,
            AcceptedCommand::ExecuteSourceExpressionCallback { .. } => 26,
            AcceptedCommand::InitializeLotPermissions { .. } => 27,
            AcceptedCommand::InitializeOwnedAdmission { .. } => 28,
            AcceptedCommand::AdmitOwnedAvatar(_) => 29,
            AcceptedCommand::SourceGoto(_) => 30,
            AcceptedCommand::RequestOwnedLeave(_) => 31,
        };
        assert_eq!(actual, expected);
        let encoded = canonical(&command);
        assert_eq!(&encoded[..4], &expected.to_le_bytes());
        if expected < 27 {
            let legacy: LegacyAcceptedCommand = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .with_little_endian()
                .allow_trailing_bytes()
                .deserialize(&bytes)
                .unwrap();
            assert_eq!(
                encoded,
                canonical(&legacy),
                "complete legacy payload for tag {expected}"
            );
            assert_eq!(format!("{command:?}"), format!("{legacy:?}"));
        }
        let decoded: AcceptedCommand = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .reject_trailing_bytes()
            .deserialize(&encoded)
            .unwrap();
        assert_eq!(decoded, command);
    }
    assert_eq!(canonical(&initialize(lot())), canonical(&(27_u32, lot())));
}

#[test]
fn nonzero_legacy_payloads_keep_complete_bytes_and_semantic_values() {
    use sim_core::{
        ids::{EntityRef, ObjectId},
        vm::{FrameContext, RoutineKey, RoutineScope},
    };
    let entity = EntityRef {
        object_id: ObjectId(123),
        generation: 456,
    };
    for command in [
        spawn(),
        AcceptedCommand::Delete { entity },
        AcceptedCommand::StartBehavior {
            entity,
            routine: RoutineKey {
                scope: RoutineScope::Private(100),
                id: 4096,
            },
            context: FrameContext::for_entity(entity, 100),
            args: vec![-32768, 42, 32767],
            replace: true,
        },
        AcceptedCommand::SetInteractionProjection {
            entity,
            queued_users: [entity].into(),
            active_advertisements: Some([((3, 1234), -101)].into()),
        },
        AcceptedCommand::SetObjectStatus {
            entity,
            disabled_flags: 56,
            broken: true,
        },
        AcceptedCommand::SetTs1FamilyBudget(Some(123456)),
        AcceptedCommand::SetInteractionAuthority {
            actor: entity,
            access: Some(sim_core::runtime::interaction_adapter::InteractionAccess {
                principal: sim_core::interactions::PrincipalKey(321),
                allow_hidden: true,
            }),
        },
        AcceptedCommand::ExecuteNamedEntryPoint {
            entity,
            name: "CT - Original callback".into(),
            args: vec![1, -2, 3],
        },
        AcceptedCommand::ExecuteSourceExpressionCallback {
            entity,
            name: "CT - Source callback".into(),
            args: vec![4, -5, 6],
        },
    ] {
        let bytes = canonical(&command);
        let legacy: LegacyAcceptedCommand = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .reject_trailing_bytes()
            .deserialize(&bytes)
            .unwrap();
        assert_eq!(canonical(&legacy), bytes);
        assert_eq!(format!("{command:?}"), format!("{legacy:?}"));
    }
}
