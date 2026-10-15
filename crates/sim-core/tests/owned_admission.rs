//! Contract harnesses only. These authored routines do not qualify original
//! avatar/controller content or establish a runnable original-object release.
use bincode::Options;
use sha2::{Digest, Sha256};
use sim_core::{
    avatars::{
        motives::TsoMotiveTuning,
        state::{AvatarPermissions, TEMPLATE_PERSON},
        AvatarPlatform, AvatarState,
    },
    ids::*,
    lot_permissions::*,
    owned_admission::*,
    runtime::*,
    state::*,
    vm::*,
    world::*,
};
use std::collections::BTreeMap;
fn runtime() -> SimRuntime {
    let tuning = TuningSet {
        tso_motives: Some(TsoMotiveTuning {
            flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
            category_weights: [[1000; 7]; 11],
        }),
        ..Default::default()
    };
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![
            ObjectDefinition::new(TEMPLATE_PERSON, 0),
            ObjectDefinition::new(100, 0),
        ],
        vec![],
        tuning,
    )
    .unwrap();
    SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap()
}
fn source(runtime: &SimRuntime) -> SourceJoinMetadata {
    SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: runtime.state().content,
        neighborhood_id: 8,
        lot_location: 16777472,
        avatar_guids: [TEMPLATE_PERSON].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    }
}
fn initialize(runtime: &mut SimRuntime, lot: LotPermissionState) {
    let tick = runtime
        .next_tick(vec![
            AcceptedCommand::InitializeLotPermissions { lot },
            AcceptedCommand::InitializeOwnedAdmission {
                source: source(runtime),
            },
        ])
        .unwrap();
    runtime.step(&tick).unwrap();
}
fn command(
    runtime: &SimRuntime,
    id: u32,
    principal: u64,
    role: AvatarPermissions,
) -> AcceptedCommand {
    let mut portable = AvatarState::new(
        EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        },
        PersistentId(id),
        AvatarPlatform::Tso,
    )
    .persistent_state()
    .unwrap();
    portable.name = format!("owned {id}");
    portable.permissions = role;
    portable.budget_mirror = 123;
    portable.person_data[0] = 321;
    portable
        .relationships
        .insert(PersistentId(900), vec![10, 20]);
    let revision = match runtime.state().owned_admission {
        OwnedAdmissionState::Initialized {
            permission_revision,
            ..
        } => permission_revision,
        _ => 1,
    };
    AcceptedCommand::AdmitOwnedAvatar(OwnedAvatarAdmission {
        control: OwnedControl {
            principal,
            account_id: 1000 + u64::from(id),
            avatar_id: PersistentId(id),
            actor: EntityRef {
                object_id: ObjectId(0),
                generation: 0,
            },
            control_generation: 1,
            session_ref: [id as u8; 32],
            admission_operation: format!("admission-{id:016}"),
            reservation_digest: [2; 32],
            export_digest: [3; 32],
        },
        portable,
        permission_revision: revision,
        permission_sha256: permission_digest(runtime.state()).unwrap_or([0; 32]),
    })
}
#[test]
fn two_owned_avatars_replay_restore_and_reconnect_without_portable_reset() {
    let mut authority = runtime();
    initialize(&mut authority, LotPermissionState::default());
    let mut replica = SimRuntime::from_state(
        authority.state().clone(),
        authority.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    for (id, principal) in [(17, 1), (18, 2)] {
        let tick = authority
            .next_tick(vec![command(
                &authority,
                id,
                principal,
                AvatarPermissions::Visitor,
            )])
            .unwrap();
        let a = authority.step(&tick).unwrap();
        let b = replica.step(&tick).unwrap();
        assert_eq!(a.state_hash, b.state_hash);
        assert_eq!(a.events, b.events);
        let actor = authority.state().owned_admission.actors().unwrap()[&principal].actor;
        assert_eq!(
            authority.state().entities[&actor.object_id]
                .info
                .persistent_id,
            id
        );
        assert_eq!(
            authority.state().interaction_access[&actor].principal.0,
            principal
        );
        assert_eq!(
            authority.state().entities[&actor.object_id]
                .avatar
                .as_ref()
                .unwrap()
                .budget_mirror,
            123
        );
        assert_eq!(
            authority.state().relationships.matrices[&RelationshipKey {
                owner: RelationshipOwner::Entity(actor),
                target: RelationshipTarget::Persistent(900)
            }],
            vec![10, 20]
        );
        let saved = authority.snapshot().unwrap();
        replica.restore(&saved).unwrap();
        assert!(replica.step(&tick).unwrap().duplicate);
        assert_eq!(replica.snapshot().unwrap(), saved);
    }
    let actors = authority.state().owned_admission.actors().unwrap();
    assert_eq!(actors.len(), 2);
    assert_ne!(actors[&1].actor, actors[&2].actor);
    let tick = authority
        .next_tick(vec![command(&authority, 17, 3, AvatarPermissions::Visitor)])
        .unwrap();
    let before = authority.snapshot().unwrap();
    assert!(authority.step(&tick).is_err());
    assert_eq!(authority.snapshot().unwrap(), before);
}
#[test]
fn admission_requires_explicit_source_state_and_fresh_permission_binding() {
    let mut runtime = runtime();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeLotPermissions {
            lot: LotPermissionState::default(),
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![command(&runtime, 17, 1, AvatarPermissions::Visitor)])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeOwnedAdmission {
            source: source(&runtime),
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    let mut wrong_kind = command(&runtime, 17, 1, AvatarPermissions::Visitor);
    if let AcceptedCommand::AdmitOwnedAvatar(a) = &mut wrong_kind {
        a.portable.custom_guid = 100;
    }
    let before_kind = runtime.snapshot().unwrap();
    let tick = runtime.next_tick(vec![wrong_kind]).unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before_kind);
    let mut c = command(&runtime, 17, 1, AvatarPermissions::Owner);
    if let AcceptedCommand::AdmitOwnedAvatar(a) = &mut c {
        a.permission_sha256 = [0; 32];
    }
    let before = runtime.snapshot().unwrap();
    let tick = runtime.next_tick(vec![c]).unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}
#[test]
fn all_source_roles_and_independent_flag_bits_are_retained() {
    let mut runtime = runtime();
    initialize(&mut runtime, LotPermissionState::default());
    let mut input = PermissionSource {
        registered_at_ms: 0,
        observed_at_ms: 8 * 86400000,
        roommate_lots: vec![],
        moderation_level: 0,
        mayor_nhood: None,
    };
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &input).unwrap(),
        (AvatarPermissions::Visitor, 1)
    );
    input.observed_at_ms = 0;
    input.roommate_lots.push(RoommateSource {
        lot_id: 42,
        permissions_level: 0,
        is_pending: false,
    });
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &input).unwrap(),
        (AvatarPermissions::Visitor, 2)
    );
    input.mayor_nhood = Some(8);
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &input)
            .unwrap()
            .1,
        6
    );
    for (level, role) in [
        (0, AvatarPermissions::Roommate),
        (1, AvatarPermissions::BuildBuyRoommate),
        (2, AvatarPermissions::Owner),
    ] {
        input.roommate_lots = vec![RoommateSource {
            lot_id: 41,
            permissions_level: level,
            is_pending: false,
        }];
        assert_eq!(
            destination_permissions(runtime.state(), PersistentId(17), &input)
                .unwrap()
                .0,
            role
        );
    }
    input.moderation_level = 1;
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &input)
            .unwrap()
            .0,
        AvatarPermissions::Admin
    );
}
#[test]
fn community_owner_donor_admin_and_metadata_revision_are_source_ordered() {
    for (incoming, expected, member) in [
        (
            AvatarPermissions::Visitor,
            AvatarPermissions::Roommate,
            true,
        ),
        (AvatarPermissions::Owner, AvatarPermissions::Owner, true),
        (AvatarPermissions::Admin, AvatarPermissions::Admin, true),
    ] {
        let mut runtime = runtime();
        initialize(
            &mut runtime,
            LotPermissionState {
                property_category: 11,
                roommates: [PersistentId(17)].into(),
                ..Default::default()
            },
        );
        let tick = runtime
            .next_tick(vec![command(&runtime, 17, 1, incoming)])
            .unwrap();
        runtime.step(&tick).unwrap();
        let actor = runtime.state().owned_admission.actors().unwrap()[&1].actor;
        assert_eq!(
            runtime.state().entities[&actor.object_id]
                .avatar
                .as_ref()
                .unwrap()
                .permissions,
            expected
        );
        let ReplicatedLotPermissions::Initialized(lot) = &runtime.state().lot_permissions else {
            panic!()
        };
        assert_eq!(lot.roommates.contains(&PersistentId(17)), member);
    }
    let mut runtime = runtime();
    initialize(&mut runtime, LotPermissionState::default());
    let before = permission_digest(runtime.state()).unwrap();
    let tick = runtime
        .next_tick(vec![command(&runtime, 17, 1, AvatarPermissions::Owner)])
        .unwrap();
    runtime.step(&tick).unwrap();
    assert_ne!(permission_digest(runtime.state()).unwrap(), before);
    assert!(matches!(
        runtime.state().owned_admission,
        OwnedAdmissionState::Initialized {
            permission_revision: 2,
            ..
        }
    ));
}
#[test]
fn malformed_registration_and_unknown_object_authority_are_atomic() {
    let mut runtime = runtime();
    let tick = runtime
        .next_tick(vec![
            AcceptedCommand::InitializeLotPermissions {
                lot: LotPermissionState::default(),
            },
            AcceptedCommand::Spawn(SpawnSpec {
                guid: 100,
                position: LotPosition::OUT_OF_WORLD,
                facing: Facing::NORTH,
                persistent_id: PersistentId(99),
                avatar: false,
            }),
        ])
        .unwrap();
    runtime.step(&tick).unwrap();
    let reference = runtime.state().entities[&ObjectId(1)].info.reference;
    let before = runtime.snapshot().unwrap();
    let mut metadata = source(&runtime);
    metadata.object_flags.insert(reference, 0);
    metadata
        .registered_groups
        .insert(PersistentId(99), vec![reference]);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeOwnedAdmission {
            source: metadata.clone(),
        }])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
    let mut state = runtime.state().clone();
    let mut object = state.world.object(reference).unwrap().clone();
    object.owner = Some(PersistentId(0));
    state.world.replace_object(object).unwrap();
    runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeOwnedAdmission {
            source: metadata,
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    let mut state = runtime.state().clone();
    if let OwnedAdmissionState::Initialized { source, .. } = &mut state.owned_admission {
        source
            .registered_groups
            .insert(PersistentId(100), vec![reference]);
    }
    assert!(
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).is_err()
    );
}
#[test]
fn schema_three_migrates_to_uninitialized_control_and_preserves_last_receipt() {
    let mut runtime = runtime();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeLotPermissions {
            lot: LotPermissionState::default(),
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    let current = runtime.snapshot().unwrap();
    let end = current.len() - 32 - 4;
    assert_eq!(&current[end..end + 4], &[0; 4]);
    let mut legacy = current[..end].to_vec();
    legacy[10..12].copy_from_slice(&3u16.to_le_bytes());
    legacy[108..110].copy_from_slice(&3u16.to_le_bytes());
    legacy[100..108].copy_from_slice(&((end - 108) as u64).to_le_bytes());
    let checksum = Sha256::digest(&legacy);
    legacy.extend_from_slice(&checksum);
    runtime.restore(&legacy).unwrap();
    assert_eq!(runtime.state().schema, 4);
    assert_eq!(
        runtime.state().owned_admission,
        OwnedAdmissionState::Uninitialized
    );
    assert!(runtime.step(&tick).unwrap().duplicate);
}
#[test]
fn appended_commands_do_not_change_tag_twenty_seven() {
    let runtime = runtime();
    for (tag, command) in [
        (
            27,
            AcceptedCommand::InitializeLotPermissions {
                lot: LotPermissionState::default(),
            },
        ),
        (
            28,
            AcceptedCommand::InitializeOwnedAdmission {
                source: source(&runtime),
            },
        ),
        (29, command(&runtime, 17, 1, AvatarPermissions::Visitor)),
    ] {
        let bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&command)
            .unwrap();
        assert_eq!(&bytes[..4], &(tag as u32).to_le_bytes());
    }
}
#[test]
fn stale_or_duplicate_control_and_double_admission_rollback_everything() {
    let mut runtime = runtime();
    initialize(&mut runtime, LotPermissionState::default());
    let first = command(&runtime, 17, 1, AvatarPermissions::Visitor);
    let before = runtime.snapshot().unwrap();
    let tick = runtime.next_tick(vec![first.clone(), first]).unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
    let mut source = source(&runtime);
    source.registered_groups.insert(
        PersistentId(100),
        vec![EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        }],
    );
    let tick = runtime
        .next_tick(vec![AcceptedCommand::InitializeOwnedAdmission { source }])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn source_avatar_init_occupation_is_seen_by_join_overbudget_reconciliation() {
    use sim_core::primitives::arithmetic::ExpressionOperand;
    let mut runtime = runtime();
    let mut content = runtime.content().clone();
    let key = RoutineKey {
        scope: RoutineScope::Private(TEMPLATE_PERSON),
        id: 4096,
    };
    let instruction = |lhs: Variable, rhs: i16, next: u8| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs,
                rhs: Variable::new(Scope::Literal, rhs),
                is_signed: 0,
                operator: 5,
            }
            .encode(),
        )
    };
    // Init changes the final registered group's occupation after admission's
    // readiness check. The source reconciliation must observe this new value.
    let mut routines = content.routines().clone();
    routines
        .insert(
            key,
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    instruction(Variable::new(Scope::StackObjectId, 0), 76, 1),
                    instruction(Variable::new(Scope::StackObject, 8), 32, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut avatar = ObjectDefinition::new(TEMPLATE_PERSON, 0);
    avatar.entry_points.insert(0, key);
    content = ContentSet::new(
        routines,
        vec![avatar, ObjectDefinition::new(100, 0)],
        vec![],
        content.tuning().clone(),
    )
    .unwrap();
    runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawn = (1..=76)
        .map(|id| {
            AcceptedCommand::Spawn(SpawnSpec {
                guid: 100,
                position: LotPosition::OUT_OF_WORLD,
                facing: Facing::NORTH,
                persistent_id: PersistentId(1000 + id),
                avatar: false,
            })
        })
        .collect();
    let tick = runtime.next_tick(spawn).unwrap();
    runtime.step(&tick).unwrap();
    let mut state = runtime.state().clone();
    for object in state.world.objects().values().cloned().collect::<Vec<_>>() {
        let mut object = object;
        object.owner = Some(PersistentId(17));
        state.world.replace_object(object).unwrap();
    }
    runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let mut metadata = source(&runtime);
    for entity in runtime.state().entities.values() {
        metadata.object_flags.insert(entity.info.reference, 0);
        metadata.registered_groups.insert(
            PersistentId(entity.info.persistent_id),
            vec![entity.info.reference],
        );
    }
    let tick = runtime
        .next_tick(vec![
            AcceptedCommand::InitializeLotPermissions {
                lot: LotPermissionState::default(),
            },
            AcceptedCommand::InitializeOwnedAdmission { source: metadata },
        ])
        .unwrap();
    runtime.step(&tick).unwrap();
    let tick = runtime
        .next_tick(vec![command(&runtime, 17, 1, AvatarPermissions::Owner)])
        .unwrap();
    runtime.step(&tick).unwrap();
    assert_eq!(
        runtime.state().entities[&ObjectId(76)].object_data[8] as u16 & 32,
        32
    );
    assert_eq!(
        runtime.state().entities[&ObjectId(76)].disabled_flags & OBJECT_LIMIT_EXCEEDED,
        OBJECT_LIMIT_EXCEEDED
    );
    assert_eq!(
        runtime.state().entities[&ObjectId(76)].disabled_flags & OBJECT_LIMIT_THREAD_DISABLE,
        0,
        "post-init occupation prevents source thread disabling"
    );
}

#[test]
fn source_permission_epoch_boundaries_and_pending_membership_are_exact() {
    let mut runtime = runtime();
    initialize(&mut runtime, LotPermissionState::default());
    let mut source = PermissionSource {
        registered_at_ms: 0,
        observed_at_ms: 7 * 86400000 - 1000,
        roommate_lots: vec![],
        moderation_level: 0,
        mayor_nhood: None,
    };
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &source)
            .unwrap()
            .1,
        3
    );
    source.observed_at_ms = 7 * 86400000;
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &source)
            .unwrap()
            .1,
        1
    );
    source.registered_at_ms = 999;
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &source)
            .unwrap()
            .1,
        1,
        "source epoch seconds ignore registration subsecond"
    );
    source.registered_at_ms = source.observed_at_ms + 1;
    assert!(destination_permissions(runtime.state(), PersistentId(17), &source).is_err());
    source.registered_at_ms = 0;
    source.observed_at_ms = 0;
    source.roommate_lots = vec![RoommateSource {
        lot_id: 41,
        permissions_level: 2,
        is_pending: true,
    }];
    assert_eq!(
        destination_permissions(runtime.state(), PersistentId(17), &source).unwrap(),
        (AvatarPermissions::Visitor, 2)
    );
}

#[test]
fn boxed_join_metadata_preserves_schema_four_bytes() {
    #[derive(serde::Serialize)]
    enum InlineState<'a> {
        Uninitialized,
        Initialized {
            source: &'a SourceJoinMetadata,
            revision: u64,
            permission_revision: u64,
            last_principal: u64,
            actors: &'a BTreeMap<u64, OwnedControl>,
        },
    }
    let mut runtime = runtime();
    initialize(&mut runtime, LotPermissionState::default());
    let state = &runtime.state().owned_admission;
    let OwnedAdmissionState::Initialized {
        source,
        revision,
        permission_revision,
        last_principal,
        actors,
    } = state
    else {
        panic!("initialized state required")
    };
    let old = InlineState::Initialized {
        source,
        revision: *revision,
        permission_revision: *permission_revision,
        last_principal: *last_principal,
        actors,
    };
    let bytes = bincode::serialize(&old).unwrap();
    assert_eq!(bytes, bincode::serialize(state).unwrap());
    assert_eq!(
        bincode::deserialize::<OwnedAdmissionState>(&bytes).unwrap(),
        *state
    );
    assert_eq!(
        bincode::serialize(&InlineState::Uninitialized).unwrap(),
        bincode::serialize(&OwnedAdmissionState::Uninitialized).unwrap()
    );
}
