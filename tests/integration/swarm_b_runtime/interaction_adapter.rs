//! The active-prefix setup is a queue fixture. Projection admission, memory
//! semantics, replica replay and snapshot validation use the actual A runtime.
use wonderland_content_runtime_bridge::{
    interaction_rules::{adapters::QueueRuntime, *},
    interactions::{self, entity_key, project_active_queues, ProjectionLimits},
    sim_core::{
        ids::{EntityRef, ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{EntityField, MemoryAddress, RoutineStore, VmMode},
        world::{Facing, LotModel, TilePos},
    },
};

fn fixture() -> (SimRuntime, [EntityRef; 4]) {
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(1, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 19, 4, 42),
        RuntimeRole::Authority,
    )
    .unwrap();
    let commands = (0..4)
        .map(|index| {
            AcceptedCommand::Spawn(SpawnSpec {
                guid: 1,
                position: TilePos::new(2 + index, 3, 1).center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(if index < 2 { index as u32 + 50 } else { 0 }),
                avatar: index < 2,
            })
        })
        .collect();
    let input = runtime.next_tick(commands).unwrap();
    runtime.step(&input).unwrap();
    let entities = [1, 2, 3, 4].map(|id| runtime.state().entities[&ObjectId(id)].info.reference);
    (runtime, entities)
}

struct QueueSetup;
impl QueueRuntime for QueueSetup {
    fn target_is_alive(&self, _: EntityKey) -> bool {
        true
    }
    fn check_action(&mut self, _: &ActionInvocation) -> Result<bool> {
        Ok(true)
    }
    fn start_action(&mut self, _: &ActionInvocation) -> Result<bool> {
        Ok(true)
    }
}

fn queue(actor: EntityRef, target: EntityRef, next: EntityRef) -> ActionQueue {
    let mut queue = ActionQueue::new(
        entity_key(actor),
        LegacyMode::Ts1,
        InteractionLimits::default(),
    )
    .unwrap();
    let definition = |index| InteractionDefinition {
        key: InteractionKey {
            scope: InteractionScope::Local,
            tta_index: index,
        },
        action: RoutineBinding {
            code_owner_guid: 1,
            routine_id: 4096,
        },
        check: None,
        flags: ActionFlags(0),
        permissions: PermissionFlags(0),
        label: Some("queue fixture".into()),
    };
    for (index, target) in [(0, target), (1, next)] {
        queue
            .enqueue_internal(
                ActionInvocation::pushed(
                    entity_key(actor),
                    entity_key(target),
                    definition(index),
                    PushPriority::UserDriven,
                    None,
                    false,
                    false,
                ),
                &QueueSetup,
            )
            .unwrap();
    }
    queue.attempt_push(&mut QueueSetup).unwrap();
    queue
}

fn use_count(runtime: &SimRuntime, target: EntityRef) -> i16 {
    wonderland_content_runtime_bridge::sim_core::runtime_memory::read_memory(
        runtime.state(),
        runtime.content(),
        &MemoryAddress::Entity {
            entity: target,
            field: EntityField::ObjectData,
            index: 62,
        },
    )
    .unwrap()
}

#[test]
fn active_prefix_projection_excludes_future_actions_and_replays_through_actual_runtime() {
    let (mut runtime, [actor, second, target, future]) = fixture();
    let first = queue(actor, target, future);
    let second = queue(second, target, future);
    let before = runtime.snapshot().unwrap();
    let projection =
        project_active_queues(&runtime, &[&first, &second], ProjectionLimits::default()).unwrap();
    assert_eq!(projection.commands().len(), 1);
    assert_eq!(runtime.snapshot().unwrap(), before);
    let input = projection.into_tick(&runtime, vec![]).unwrap();
    let mut replica = SimRuntime::from_state(
        runtime.state().clone(),
        runtime.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    let live = runtime.step(&input).unwrap();
    let replay = replica.step(&input).unwrap();
    assert_eq!(live.state_hash, replay.state_hash);
    assert_eq!(use_count(&runtime, target), 2);
    assert_eq!(use_count(&runtime, future), 0);
    assert!(replay.effects.is_empty());
    let clear = project_active_queues(&runtime, &[], ProjectionLimits::default())
        .unwrap()
        .into_tick(&runtime, vec![])
        .unwrap();
    runtime.step(&clear).unwrap();
    assert_eq!(use_count(&runtime, target), 0);
}

#[test]
fn projection_rejects_duplicate_owners_stale_generations_and_excess_before_commit() {
    let (runtime, [actor, _, target, future]) = fixture();
    let first = queue(actor, target, future);
    let before = runtime.snapshot().unwrap();
    assert!(
        project_active_queues(&runtime, &[&first, &first], ProjectionLimits::default()).is_err()
    );
    let stale = queue(
        EntityRef {
            generation: actor.generation + 1,
            ..actor
        },
        target,
        future,
    );
    assert!(project_active_queues(&runtime, &[&stale], ProjectionLimits::default()).is_err());
    let stale_target = queue(
        actor,
        EntityRef {
            generation: target.generation + 1,
            ..target
        },
        future,
    );
    assert!(
        project_active_queues(&runtime, &[&stale_target], ProjectionLimits::default()).is_err()
    );
    assert!(project_active_queues(
        &runtime,
        &[&first],
        ProjectionLimits {
            max_bytes: 1,
            ..ProjectionLimits::default()
        }
    )
    .is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn prepared_projection_cannot_be_reused_after_the_runtime_advances() {
    let (mut runtime, [actor, _, target, future]) = fixture();
    let queue = queue(actor, target, future);
    let projection =
        project_active_queues(&runtime, &[&queue], ProjectionLimits::default()).unwrap();
    let next = runtime.next_tick(vec![]).unwrap();
    runtime.step(&next).unwrap();
    assert!(projection.into_tick(&runtime, vec![]).is_err());
    assert!(interactions::entity_ref(EntityKey {
        slot: 32768,
        generation: 1
    })
    .is_err());
}

struct Authority;
impl interactions::InteractionAuthority for Authority {
    fn authorize(
        &self,
        principal: PrincipalKey,
        _: EntityKey,
        _: interaction_rules::adapters::AuthorityOperation,
    ) -> bool {
        principal.0 == 77
    }
    fn owns_target(&self, _: EntityRef, _: EntityRef) -> Option<bool> {
        None
    }
}

fn check_fixture() -> (
    SimRuntime,
    [EntityRef; 4],
    wonderland_content_runtime_bridge::content::ImportedContent,
) {
    use wonderland_content_runtime_bridge::{
        content::{ImportedContent, ImportedInteraction, ImportedObject},
        sim_core::vm::{RoutineKey, RoutineScope, VmInstruction, VmRoutine},
    };
    let (runtime, entities) = fixture();
    let mut routines = RoutineStore::new();
    let key = |id| RoutineKey {
        scope: RoutineScope::Private(1),
        id,
    };
    for (id, instructions) in [
        (
            4096,
            vec![VmInstruction::new(2, 254, 255, [1, 0, 1, 0, 0, 2, 7, 7])],
        ),
        (4097, vec![VmInstruction::new(4098, 254, 255, [0; 8])]),
        (
            4098,
            vec![VmInstruction::new(2, 254, 255, [50, 0, 0, 0, 0, 2, 3, 7])],
        ),
        (
            4099,
            vec![VmInstruction::new(2, 254, 255, [0, 0, 7, 0, 0, 5, 8, 7])],
        ),
        (4100, vec![VmInstruction::new(4099, 254, 255, [0; 8])]),
        (
            4101,
            vec![VmInstruction::new(32, 254, 255, [1, 0, 0, 0, 0, 0, 10, 0])],
        ),
        (
            4102,
            vec![VmInstruction::new(8, 254, 255, [0, 0, 8, 0, 8, 0, 0, 0])],
        ),
        (
            4103,
            vec![
                // Only the target's raw Occupied bit blocks the offer. Source
                // intent validation clears it while retaining active UseCount.
                VmInstruction::new(2, 255, 1, [8, 0, 6, 0, 0, 8, 4, 7]),
                VmInstruction::new(2, 254, 255, [62, 0, 1, 0, 0, 2, 4, 7]),
            ],
        ),
    ] {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, instructions).unwrap())
            .unwrap();
    }
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(1, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let imported = ImportedContent {
        content: content.clone(),
        objects: vec![ImportedObject {
            guid: 1,
            object_chunk_id: 1,
            source_name: "declared check fixture".into(),
            effective_identity: [5; 32],
            semiglobal_owner: None,
            interactions: Some(vec![ImportedInteraction {
                tta_index: 900,
                action: key(4096),
                check: Some(key(4097)),
                code_owner: 1,
                flags: 1,
                permissions: 0,
                label: Some("Source comparison".into()),
            }]),
        }],
    };
    let mut state = runtime.state().clone();
    state.content = content.descriptor().unwrap();
    state
        .entities
        .get_mut(&entities[0].object_id)
        .unwrap()
        .object_data[50] = 1;
    (
        SimRuntime::from_state(state, content, RuntimeRole::Authority).unwrap(),
        entities,
        imported,
    )
}

#[test]
fn real_read_only_check_provider_resolves_call_closure_and_preserves_ui_state() {
    use interaction_rules::adapters::WorldProvider;
    let (runtime, [actor, _, target, _], imported) = check_fixture();
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let query = OfferQuery {
        principal: PrincipalKey(77),
        seen: snapshot.stamp(),
        options: QueryOptions::default(),
    };
    let offered = query_offers(&world, &checks, query, &limits).unwrap();
    assert_eq!(offered.offers.len(), 1);
    assert_eq!(offered.offers[0].interaction.tta_index, 900);
    assert!(offered.offers[0].advertisements.is_empty());
    assert_eq!(runtime.snapshot().unwrap(), before);
    let mut detached = snapshot;
    query_in_tick(&mut detached, &checks, QueryOptions::default(), &limits).unwrap();
    assert_eq!(detached.state().rng_seed, runtime.state().rng.state());
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn write_or_random_check_closures_are_rejected_before_interpretation() {
    use interaction_rules::adapters::WorldProvider;
    let (runtime, [actor, _, target, _], mut imported) = check_fixture();
    let limits = InteractionLimits::default();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let before = runtime.snapshot().unwrap();
    for id in [4100, 4102] {
        imported.objects[0].interactions.as_mut().unwrap()[0]
            .check
            .as_mut()
            .unwrap()
            .id = id;
        let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
        let world =
            interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
        let snapshot = world
            .snapshot(entity_key(actor), entity_key(target), &limits)
            .unwrap();
        let query = OfferQuery {
            principal: PrincipalKey(77),
            seen: snapshot.stamp(),
            options: QueryOptions::default(),
        };
        assert!(matches!(
            query_offers(&world, &checks, query, &limits),
            Err(Error::Unsupported(_))
        ));
    }
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn read_only_object_type_checks_use_actual_target_guid() {
    use interaction_rules::adapters::WorldProvider;
    let (runtime, [actor, _, target, _], mut imported) = check_fixture();
    imported.objects[0].interactions.as_mut().unwrap()[0]
        .check
        .as_mut()
        .unwrap()
        .id = 4101;
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let offered = query_offers(
        &world,
        &checks,
        OfferQuery {
            principal: PrincipalKey(77),
            seen: snapshot.stamp(),
            options: QueryOptions::default(),
        },
        &limits,
    )
    .unwrap();
    assert_eq!(offered.offers.len(), 1);
}

#[test]
fn target_broken_fact_comes_from_the_actual_multitile_base() {
    use interaction_rules::adapters::WorldProvider;
    let (runtime, [actor, _, target, base], mut imported) = check_fixture();
    imported.objects[0].interactions.as_mut().unwrap()[0].check = None;
    let mut state = runtime.state().clone();
    state.mode = VmMode::Tso;
    for item in state.entities.values_mut() {
        if let Some(avatar) = &mut item.avatar {
            avatar.platform =
                wonderland_content_runtime_bridge::sim_core::avatars::AvatarPlatform::Tso;
            avatar.decay =
                wonderland_content_runtime_bridge::sim_core::avatars::motives::MotiveDecay::tso(
                    None,
                );
        }
    }
    for thread in state.threads.values_mut() {
        thread.mode = VmMode::Tso;
    }
    for entity in [target, base] {
        let item = state.entities.get_mut(&entity.object_id).unwrap();
        item.info.base_object = base.object_id;
        item.info.multi_tile = true;
        item.info.group = vec![target.object_id, base.object_id];
        let mut projection = state.world.object(entity).unwrap().clone();
        projection.multitile_group = Some(base);
        state.world.replace_object(projection).unwrap();
    }
    state.entities.get_mut(&base.object_id).unwrap().broken = true;
    let runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    struct OwnedView;
    impl interactions::InteractionAuthority for OwnedView {
        fn authorize(
            &self,
            _: PrincipalKey,
            _: EntityKey,
            _: interaction_rules::adapters::AuthorityOperation,
        ) -> bool {
            true
        }
        fn owns_target(&self, _: EntityRef, _: EntityRef) -> Option<bool> {
            Some(false)
        }
    }
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &OwnedView, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let offered = query_offers(
        &world,
        &checks,
        OfferQuery {
            principal: PrincipalKey(77),
            seen: snapshot.stamp(),
            options: QueryOptions::default(),
        },
        &limits,
    )
    .unwrap();
    assert!(
        offered.offers.is_empty(),
        "a broken base suppresses a normal private action on its wing"
    );
}

#[test]
fn runtime_offer_snapshots_bound_source_bytes_and_enforce_real_generation_and_authority() {
    use interaction_rules::adapters::WorldProvider;
    let (runtime, [actor, _, target, _], imported) = check_fixture();
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    assert!(world
        .snapshot(
            entity_key(actor),
            entity_key(target),
            &InteractionLimits {
                max_provider_state_bytes: 1,
                ..limits
            }
        )
        .is_err());
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let query = OfferQuery {
        principal: PrincipalKey(0),
        seen: snapshot.stamp(),
        options: QueryOptions::default(),
    };
    assert_eq!(
        query_offers(&world, &checks, query, &limits).unwrap_err(),
        Error::Unauthorized
    );
    let stale = EntityKey {
        generation: actor.generation + 1,
        ..entity_key(actor)
    };
    assert!(world.snapshot(stale, entity_key(target), &limits).is_err());
}

#[test]
fn global_table_bindings_are_scoped_to_the_target_object_code_owner() {
    use interaction_rules::adapters::WorldProvider;
    use wonderland_content_runtime_bridge::{
        content::{ImportedInteraction, ImportedObject},
        sim_core::vm::{RoutineKey, RoutineScope, VmInstruction, VmRoutine},
    };
    let (runtime, [actor, _, target, _], mut imported) = check_fixture();
    let mut routines = runtime.content().routines().clone();
    let global = RoutineKey {
        scope: RoutineScope::Global,
        id: 256,
    };
    routines
        .insert(
            global,
            VmRoutine::new(
                256,
                0,
                4,
                vec![VmInstruction::new(2, 254, 255, [1, 0, 1, 0, 0, 2, 7, 7])],
            )
            .unwrap(),
        )
        .unwrap();
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(1, 0), ObjectDefinition::new(2, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    imported.content = content.clone();
    imported.objects.push(ImportedObject {
        guid: 2,
        object_chunk_id: 2,
        source_name: "second source object".into(),
        effective_identity: [6; 32],
        semiglobal_owner: None,
        interactions: None,
    });
    let mut state = runtime.state().clone();
    state.content = content.descriptor().unwrap();
    let runtime = SimRuntime::from_state(state, content, RuntimeRole::Authority).unwrap();
    let globals = [ImportedInteraction {
        tta_index: 501,
        action: global,
        check: None,
        code_owner: 2,
        flags: 1,
        permissions: 0,
        label: Some("second owner only".into()),
    }];
    let limits = InteractionLimits::default();
    let catalog =
        interactions::RuntimeCatalog::from_imported(&imported, &globals, &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let offered = query_offers(
        &world,
        &checks,
        OfferQuery {
            principal: PrincipalKey(77),
            seen: snapshot.stamp(),
            options: QueryOptions::default(),
        },
        &limits,
    )
    .unwrap();
    assert_eq!(
        offered
            .offers
            .iter()
            .map(|offer| offer.interaction.tta_index)
            .collect::<Vec<_>>(),
        [900]
    );
}

fn foreign_root_snapshot(check_id: u16) -> (SimRuntime, EntityRef, EntityRef, InteractionSnapshot) {
    use wonderland_content_runtime_bridge::sim_core::vm::{
        RoutineKey, RoutineScope, VmInstruction, VmRoutine,
    };
    let (runtime, [actor, _, target, _]) = fixture();
    let literal = |right| VmInstruction::new(2, 254, 255, [1, 0, right, 0, 0, 2, 7, 7]);
    let mut routines = RoutineStore::new();
    for (owner, id, instruction) in [
        (1, 4096, literal(1)),
        (2, 4096, literal(2)),
        (1, 4097, literal(1)),
        (2, 4097, VmInstruction::new(4098, 254, 255, [0; 8])),
        (1, 4098, literal(2)),
        (2, 4098, literal(1)),
        (1, 4100, literal(1)),
        (2, 4100, VmInstruction::new(4101, 254, 255, [0; 8])),
        (
            1,
            4101,
            VmInstruction::new(2, 254, 255, [7, 0, 42, 0, 0, 5, 3, 7]),
        ),
        (2, 4101, literal(1)),
    ] {
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::Private(owner),
                    id,
                },
                VmRoutine::new(id, 0, 4, vec![instruction]).unwrap(),
            )
            .unwrap();
    }
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(1, 0), ObjectDefinition::new(2, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut state = runtime.state().clone();
    state.content = content.descriptor().unwrap();
    let runtime = SimRuntime::from_state(state, content, RuntimeRole::Authority).unwrap();
    let limits = InteractionLimits::default();
    let mut state = CheckState::default();
    state.rng_seed = runtime.state().rng.state();
    state
        .set_provider_state_owned(runtime.snapshot().unwrap(), &limits)
        .unwrap();
    let version = |entity: EntityRef| EntityVersion {
        key: entity_key(entity),
        revision: runtime.state().entities[&entity.object_id].revision,
    };
    let mut snapshot = InteractionSnapshot::new(
        LegacyMode::Ts1,
        1,
        ActorFacts {
            version: version(actor),
            is_avatar: true,
            species: Species::Human,
            permission: AvatarPermission::Visitor,
            carrying: false,
            ghost: false,
            owns_target: false,
            ts1_ungreeted_visitor: false,
            age: 25,
        },
        TargetFacts {
            version: version(target),
            is_game_object: true,
            broken: false,
            disabled: false,
        },
        state,
        &limits,
    )
    .unwrap();
    snapshot
        .add_definition(
            InteractionDefinition {
                key: InteractionKey {
                    scope: InteractionScope::Local,
                    tta_index: 1,
                },
                action: RoutineBinding {
                    routine_id: 4096,
                    code_owner_guid: 1,
                },
                check: Some(RoutineBinding {
                    routine_id: check_id,
                    code_owner_guid: 2,
                }),
                flags: ActionFlags(0),
                permissions: PermissionFlags(0),
                label: None,
            },
            &limits,
        )
        .unwrap();
    (runtime, actor, target, snapshot)
}

#[test]
fn declared_check_root_can_differ_from_action_code_owner_without_rebinding_nested_calls() {
    use wonderland_content_runtime_bridge::sim_core::vm::{
        FrameContext, PrimitiveExit, RoutineKey, RoutineScope, VmStop,
    };
    for check_id in [4096, 4097] {
        let (runtime, actor, target, mut snapshot) = foreign_root_snapshot(check_id);
        let before = runtime.snapshot().unwrap();
        let result = runtime
            .query_behavior(
                actor,
                RoutineKey {
                    scope: RoutineScope::Private(2),
                    id: check_id,
                },
                FrameContext {
                    caller: actor,
                    callee: target,
                    stack_object: target.object_id,
                    stack_object_ref: Some(target),
                    code_owner: 1,
                },
                vec![0; 4],
                100,
            )
            .unwrap();
        assert_eq!(result.stop, VmStop::Completed(PrimitiveExit::ReturnFalse));
        let checks = interactions::ReadOnlyChecks::new(
            runtime.content(),
            runtime.state().lot_id,
            runtime.state().authority_epoch,
        );
        let offers = query_in_tick(
            &mut snapshot,
            &checks,
            QueryOptions::default(),
            &InteractionLimits::default(),
        )
        .unwrap();
        assert!(
            offers.offers.is_empty(),
            "declared root {check_id} must match the actual A result"
        );
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}

#[test]
fn foreign_check_root_proof_follows_action_owner_calls_and_rejects_world_writes() {
    let (runtime, _, _, mut snapshot) = foreign_root_snapshot(4100);
    let before = runtime.snapshot().unwrap();
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    assert!(matches!(
        query_in_tick(
            &mut snapshot,
            &checks,
            QueryOptions::default(),
            &InteractionLimits::default()
        ),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn intent_validation_clears_only_the_detached_target_occupied_flag_and_retains_use_count() {
    use interaction_rules::adapters::WorldProvider;
    let (mut runtime, [actor, _, target, _], mut imported) = check_fixture();
    imported.objects[0].interactions.as_mut().unwrap()[0]
        .check
        .as_mut()
        .unwrap()
        .id = 4103;
    let input = runtime
        .next_tick(vec![
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: target,
                    field: EntityField::ObjectData,
                    index: 8,
                },
                value: (1 << 5) | (1 << 8),
            },
            AcceptedCommand::SetInteractionProjection {
                entity: target,
                queued_users: [actor].into(),
                active_advertisements: None,
            },
        ])
        .unwrap();
    runtime.step(&input).unwrap();
    let before = runtime.snapshot().unwrap();
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    assert!(snapshot.state().target_occupied);
    let checks = interactions::ReadOnlyChecks::new(
        runtime.content(),
        runtime.state().lot_id,
        runtime.state().authority_epoch,
    );
    let shown = query_offers(
        &world,
        &checks,
        OfferQuery {
            principal: PrincipalKey(77),
            seen: snapshot.stamp(),
            options: QueryOptions::default(),
        },
        &limits,
    )
    .unwrap();
    assert!(
        shown.offers.is_empty(),
        "normal menu sees raw occupied flag"
    );
    let queue = ActionQueue::new(entity_key(actor), LegacyMode::Ts1, limits).unwrap();
    assert!(
        validate_intent(
            &world,
            &checks,
            &queue,
            InteractionIntent {
                principal: PrincipalKey(77),
                seen: snapshot.stamp(),
                queue_revision: queue.revision(),
                command_sequence: 1,
                interaction: InteractionKey {
                    scope: InteractionScope::Local,
                    tta_index: 900
                },
                param0: 0,
            },
            &limits
        )
        .is_ok(),
        "network verification temporarily clears only Occupied and still sees UseCount=1"
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn snapshot_occupied_is_the_raw_target_bit_and_not_another_avatars_using_frame() {
    use interaction_rules::adapters::WorldProvider;
    use wonderland_content_runtime_bridge::sim_core::vm::FrameContext;
    let (runtime, [actor, other, target, _], imported) = check_fixture();
    let mut state = runtime.state().clone();
    state
        .entities
        .get_mut(&target.object_id)
        .unwrap()
        .queued_users
        .insert(actor);
    let routine = runtime.content().routines().resolve(1, 4096).unwrap();
    state
        .threads
        .get_mut(&other.object_id)
        .unwrap()
        .push_entry(
            runtime.content().routines(),
            routine,
            FrameContext {
                caller: other,
                callee: target,
                stack_object: target.object_id,
                stack_object_ref: Some(target),
                code_owner: 1,
            },
            vec![0; 4],
        )
        .unwrap();
    let runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    assert!(
        wonderland_content_runtime_bridge::sim_core::runtime_memory::is_in_use(
            runtime.state(),
            target,
            None
        )
        .unwrap()
    );
    let limits = InteractionLimits::default();
    let catalog = interactions::RuntimeCatalog::from_imported(&imported, &[], &limits).unwrap();
    let world =
        interactions::RuntimeInteractionWorld::new(&runtime, &catalog, &Authority, 19).unwrap();
    let snapshot = world
        .snapshot(entity_key(actor), entity_key(target), &limits)
        .unwrap();
    assert!(
        !snapshot.state().target_occupied,
        "source Verify toggles only the selected target's raw bit"
    );
}
