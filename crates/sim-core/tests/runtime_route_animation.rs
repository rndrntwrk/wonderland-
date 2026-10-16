//! Authored metadata vectors test source mechanics; these are not original clip
//! payloads and do not qualify original-content rendering.
use sim_core::{
    avatars::timeline::AnimationMetadata,
    ids::{EntityRef, ObjectId, PersistentId},
    route_motion::{
        MotionAnimation, MotionPhase, MotionResources, RouteMotionRecord, SourceLine,
        SourceRouteMotion,
    },
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    snapshot::{self, SnapshotExpectation},
    state::{AnimationKey, ContentSet, ObjectDefinition, TuningSet},
    vm::{EntityField, MemoryAddress, RoutineStore, VmMode},
    world::{Facing, LotModel, LotPosition, RouteGoal},
};

fn actor() -> EntityRef {
    EntityRef {
        object_id: ObjectId(1),
        generation: 1,
    }
}
fn content(clips: bool, run: bool) -> ContentSet {
    let mut object = ObjectDefinition::new(100, 0);
    object.object_data[17] = i16::from(run);
    object.object_data[4] = 1;
    object.object_data[42] = 3 | (1 << 11); // fixture may cross pool boundaries
    let mut table = vec![String::new(); 26];
    for i in [3, 4, 5, 6, 7, 8, 9, 20, 21, 25] {
        table[i] = format!("source-vector-{i}");
    }
    let metadata = if clips {
        table
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.is_empty())
            .map(|(i, n)| {
                (
                    AnimationKey {
                        owner: 1,
                        scope: 999,
                        id: i as u16,
                    },
                    AnimationMetadata {
                        resource: format!("{n}.anim"),
                        num_frames: if i < 10 { 4 } else { 20 },
                        time_properties: vec![],
                    },
                )
            })
            .collect()
    } else {
        vec![]
    };
    ContentSet::new(
        RoutineStore::new(),
        vec![object],
        metadata,
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![
        ((0, 150), table.clone()),
        ((0, 151), table.clone()),
        ((0, 158), table.clone()),
        ((0, 160), table),
    ])
    .unwrap()
}
fn runtime(clips: bool, run: bool, facing: Facing) -> SimRuntime {
    runtime_on_lot(clips, run, facing, LotModel::new(8, 8, 1).unwrap())
}
fn runtime_on_lot(clips: bool, run: bool, facing: Facing, lot: LotModel) -> SimRuntime {
    let mut sim = SimRuntime::new(
        content(clips, run),
        lot,
        RuntimeConfig::new(VmMode::Ts1, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    tick(
        &mut sim,
        vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 100,
            position: LotPosition::new(24, 24, 1),
            facing,
            persistent_id: PersistentId(42),
            avatar: true,
        })],
    );
    sim
}
fn tick(sim: &mut SimRuntime, commands: Vec<AcceptedCommand>) -> Vec<RuntimeEvent> {
    let input = sim.next_tick(commands).unwrap();
    sim.step(&input).unwrap().events
}
fn begin(sim: &mut SimRuntime, goal: RouteGoal) {
    tick(
        sim,
        vec![AcceptedCommand::BeginRoute {
            entity: actor(),
            target: None,
            goals: vec![goal],
        }],
    );
}
fn resources() -> MotionResources {
    MotionResources {
        names: std::array::from_fn(|i| format!("vector-{i}.anim")),
        turn_frames: [4; 6],
        walk_style: 0,
        in_pool: false,
        pet_slow: false,
        is_pet: false,
    }
}

#[test]
fn original_line_integer_units_truncation_and_speed_factors() {
    let mut line = SourceLine::new(
        LotPosition::new(24, 24, 1),
        LotPosition::new(28, 28, 1),
        false,
        &resources(),
    );
    assert_eq!(line.total_frames, 50); // floor(sqrt(4²+4²))*10, not floor(sqrt(...)*10)
    line.frames = 7;
    assert_eq!(line.position(), LotPosition::new(24, 24, 1));
    assert_eq!(line.visual()[0], 24.56 / 16.);
    let mut reverse = SourceLine::new(
        LotPosition::new(28, 28, 1),
        LotPosition::new(24, 24, 1),
        false,
        &resources(),
    );
    reverse.frames = 7;
    assert_eq!(reverse.position(), LotPosition::new(27, 27, 1));
    let mut config = resources();
    config.walk_style = 1;
    assert_eq!(
        SourceLine::new(line.from, line.to, false, &config).total_frames,
        16
    );
    config.walk_style = 0;
    config.in_pool = true;
    assert_eq!(
        SourceLine::new(line.from, line.to, false, &config).total_frames,
        100
    );
    config.in_pool = false;
    config.pet_slow = true;
    config.is_pet = true;
    assert_eq!(
        SourceLine::new(line.from, line.to, false, &config).total_frames,
        100
    );
}
#[test]
fn turn_threshold_direction_tweak_and_source_acceleration() {
    use std::f64::consts::*;
    assert_eq!(SourceRouteMotion::turn_selection(FRAC_PI_4 - 0.02), None);
    assert_eq!(SourceRouteMotion::turn_selection(FRAC_PI_4).unwrap().0, 4);
    assert_eq!(SourceRouteMotion::turn_selection(-FRAC_PI_4).unwrap().0, 5);
    assert_eq!(SourceRouteMotion::turn_selection(FRAC_PI_2).unwrap().0, 2);
    assert_eq!(SourceRouteMotion::turn_selection(-FRAC_PI_2).unwrap().0, 3);
    assert_eq!(SourceRouteMotion::turn_selection(PI).unwrap(), (0, 0.));
    assert_eq!(SourceRouteMotion::turn_selection(-PI).unwrap(), (1, -0.));
    let mut motion = SourceRouteMotion::new(
        actor(),
        LotPosition::new(24, 24, 1),
        Facing::EAST,
        resources(),
    );
    assert!(motion.begin_line(
        LotPosition::new(24, 24, 1),
        LotPosition::new(56, 24, 1),
        true
    ));
    let mut frames = 0;
    for expected in 1..=8 {
        motion.advance_line();
        frames += expected;
        assert_eq!(motion.blend_velocity, expected as u8);
        assert_eq!(motion.line.as_ref().unwrap().frames, frames);
    }
    motion.wait_tick();
    assert_eq!(motion.velocity, 7);
    assert_eq!(motion.blend_velocity, 7);
}
#[test]
fn authoritative_walk_turns_snapshot_replay_and_final_standing() {
    let mut authority = runtime(true, false, Facing::NORTH);
    let goal = RouteGoal {
        facing: Some(Facing::WEST),
        ..RouteGoal::point(LotPosition::new(56, 24, 1))
    };
    begin(&mut authority, goal);
    let mut replica = SimRuntime::from_state(
        authority.state().clone(),
        authority.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    let mut saw_begin = false;
    let mut saw_end = false;
    let mut saw_walk = false;
    let mut saw_fractional = false;
    for _ in 0..250 {
        let input = authority.next_tick(vec![]).unwrap();
        let a = authority.step(&input).unwrap();
        let b = replica.step(&input).unwrap();
        assert_eq!(a.state_hash, b.state_hash);
        assert_eq!(a.events, b.events);
        assert_eq!(authority.state(), replica.state());
        if let Some(RouteMotionRecord::Active(m)) =
            authority.state().source_routing.routes.values().next()
        {
            saw_begin |= m.phase == MotionPhase::BeginTurn;
            saw_end |= m.phase == MotionPhase::EndTurn;
            if m.animation == MotionAnimation::Walk {
                saw_walk = true;
                let layers = &authority.state().entities[&actor().object_id]
                    .avatar
                    .as_ref()
                    .unwrap()
                    .animations
                    .animations;
                assert_eq!(layers.len(), 3);
                assert_eq!(layers[1].speed, 1.5);
                assert_eq!(layers[2].speed, 1.5);
                assert!((layers[1].weight - f32::from(m.blend_velocity) / 8. * 0.66).abs() < 1e-6);
                saw_fractional |=
                    (m.visual.visual_position[0] * 16. - m.visual.position.x as f32).abs() > 0.01;
            }
            let snap = authority.snapshot().unwrap();
            replica.restore(&snap).unwrap();
            assert_eq!(replica.state(), authority.state());
        }
        if authority.state().continuations.is_empty() {
            assert!(saw_begin && saw_walk && saw_end && saw_fractional);
            let entity = &authority.state().entities[&actor().object_id];
            assert_eq!(
                (
                    entity.info.position.x,
                    entity.info.position.y,
                    entity.info.direction
                ),
                (56, 24, 6)
            );
            let layers = &entity.avatar.as_ref().unwrap().animations.animations;
            assert_eq!(layers.len(), 1);
            assert_eq!(layers[0].metadata.resource, "source-vector-3.anim");
            assert_eq!(layers[0].weight, 1.);
            return;
        }
    }
    panic!("source route did not complete");
}
#[test]
fn missing_clips_preserve_movement_only_and_name_the_missing_resource() {
    let mut sim = runtime(false, false, Facing::EAST);
    let events = tick(
        &mut sim,
        vec![AcceptedCommand::BeginRoute {
            entity: actor(),
            target: None,
            goals: vec![RouteGoal::point(LotPosition::new(56, 24, 1))],
        }],
    );
    let mut diagnostics = events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::RouteAnimationUnavailable { .. }))
        .count();
    for _ in 0..30 {
        for event in tick(&mut sim, vec![]) {
            if let RuntimeEvent::RouteAnimationUnavailable { reason, .. } = event {
                diagnostics += 1;
                assert!(reason.contains("source-vector-3.anim"));
            }
        }
        if sim.state().continuations.is_empty() {
            break;
        }
    }
    assert_eq!(diagnostics, 1);
    assert!(sim.state().continuations.is_empty());
    assert_eq!(sim.state().entities[&actor().object_id].info.position.x, 56);
    assert!(sim.state().entities[&actor().object_id]
        .avatar
        .as_ref()
        .unwrap()
        .animations
        .animations
        .is_empty());
}
#[test]
fn run_is_source_timed_and_faster_without_changing_layer_speed() {
    let complete = |run| {
        let mut sim = runtime(true, run, Facing::EAST);
        begin(&mut sim, RouteGoal::point(LotPosition::new(88, 24, 1)));
        let mut seen = false;
        for ticks in 1..250 {
            tick(&mut sim, vec![]);
            if let Some(RouteMotionRecord::Active(m)) =
                sim.state().source_routing.routes.values().next()
            {
                if m.animation == MotionAnimation::Walk {
                    seen = true;
                    assert_eq!(
                        m.resources.names[1],
                        format!("source-vector-{}.anim", if run { 21 } else { 20 })
                    );
                }
            }
            if sim.state().continuations.is_empty() {
                assert!(seen);
                return ticks;
            }
        }
        panic!("route unfinished")
    };
    assert!(complete(true) < complete(false));
}
#[test]
fn cancellation_clears_route_layers_and_keeps_other_actor_untouched() {
    let mut sim = runtime(true, false, Facing::EAST);
    tick(
        &mut sim,
        vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 100,
            position: LotPosition::new(24, 72, 1),
            facing: Facing::NORTH,
            persistent_id: PersistentId(43),
            avatar: true,
        })],
    );
    begin(&mut sim, RouteGoal::point(LotPosition::new(88, 24, 1)));
    for _ in 0..10 {
        tick(&mut sim, vec![]);
    }
    let untouched = sim.state().entities[&ObjectId(2)]
        .avatar
        .as_ref()
        .unwrap()
        .animations
        .clone();
    let flags = sim.state().entities[&actor().object_id].object_data[8] | (1 << 7);
    let events = tick(
        &mut sim,
        vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor(),
                field: EntityField::ObjectData,
                index: 8,
            },
            value: flags,
        }],
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RouteFinished { success: false, .. })));
    assert!(sim.state().source_routing.routes.is_empty());
    assert_eq!(
        sim.state().entities[&ObjectId(2)]
            .avatar
            .as_ref()
            .unwrap()
            .animations,
        untouched
    );
    assert_eq!(
        sim.state().entities[&actor().object_id]
            .avatar
            .as_ref()
            .unwrap()
            .animations
            .animations
            .len(),
        1
    );
}
#[test]
fn schema_four_migrates_empty_maps_without_inventing_inflight_timing() {
    use sha2::{Digest, Sha256};
    let mut sim = runtime(false, false, Facing::EAST);
    // A genuine schema4 body has no trailing source state. Start with no routes.
    let frame = sim.snapshot().unwrap();
    let end = frame.len() - 32 - 16 - 17;
    assert_eq!(&frame[end..end + 16], &[0; 16]);
    let mut legacy = frame[..end].to_vec();
    legacy[10..12].copy_from_slice(&4u16.to_le_bytes());
    legacy[108..110].copy_from_slice(&4u16.to_le_bytes());
    legacy[100..108].copy_from_slice(&((end - 108) as u64).to_le_bytes());
    let sum = Sha256::digest(&legacy);
    legacy.extend_from_slice(&sum);
    sim.restore(&legacy).unwrap();
    assert_eq!(sim.state().schema, sim_core::state::SIMULATION_SCHEMA);
    assert!(!sim.state().owned_departures.eod_known_clear);
    assert!(sim.state().source_routing.routes.is_empty());
    assert_eq!(
        snapshot::decode(
            &sim.snapshot().unwrap(),
            sim.content(),
            SnapshotExpectation::new(41, 7)
        )
        .unwrap(),
        *sim.state()
    );
}

#[test]
fn negative_axes_finish_the_actual_line_not_the_first_truncated_goal_coordinate() {
    for goal in [
        // Endpoints on the existing 4-subtile grid make the FINAL segment
        // west/north-only. A tile-center goal may have a positive final axis.
        LotPosition::new(10, 26, 1),
        LotPosition::new(26, 10, 1),
        LotPosition::new(10, 10, 1),
    ] {
        let mut sim = runtime(true, false, Facing::WEST);
        begin(&mut sim, RouteGoal::point(goal));
        let mut crossed_integer_goal_early = false;
        for _ in 0..200 {
            tick(&mut sim, vec![]);
            if let Some(RouteMotionRecord::Active(m)) =
                sim.state().source_routing.routes.values().next()
            {
                if m.visual.position == goal
                    && m.line.as_ref().is_some_and(|l| l.frames < l.total_frames)
                {
                    crossed_integer_goal_early = true;
                }
            }
            if sim.state().continuations.is_empty() {
                break;
            }
        }
        assert!(
            crossed_integer_goal_early,
            "negative vector {goal:?} must exercise early integer truncation"
        );
        assert!(sim.state().continuations.is_empty());
        let visual = &sim.state().source_routing.avatars[&actor()];
        assert_eq!(visual.position, goal);
        assert_eq!(visual.visual_position[0], goal.x as f32 / 16.);
        assert_eq!(visual.visual_position[1], goal.y as f32 / 16.);
    }
}
#[test]
fn source_double_pi_half_notch_and_first_turn_tweak_are_preserved() {
    assert_eq!(
        sim_core::route_motion::facing(std::f32::consts::FRAC_PI_4 / 2.),
        Facing(1)
    );
    let mut m = SourceRouteMotion::new(
        actor(),
        LotPosition::new(24, 24, 1),
        Facing::NORTH,
        resources(),
    );
    // Integer endpoint gives a turn between45 and90 degrees, with a nonzero tweak.
    assert!(!m.begin_line(
        LotPosition::new(24, 24, 1),
        LotPosition::new(31, 20, 1),
        true
    ));
    let first = m.turn_tweak;
    assert!(first > 0.);
    assert_eq!(m.radians, first);
    for _ in 1..4 {
        assert!(!m.turn_tick(false));
    }
    assert!((m.radians - first * 4.).abs() < 1e-6);
    assert!(m.turn_tick(true));
    assert_eq!(m.radians, m.target_radians as f32);
}
#[test]
fn forged_source_timing_and_pose_ownership_are_rejected_without_panics() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(88, 24, 1)));
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    for case in 0..8 {
        let mut state = sim.state().clone();
        let RouteMotionRecord::Active(m) = state.source_routing.routes.values_mut().next().unwrap()
        else {
            panic!()
        };
        match case {
            0 => m.line.as_mut().unwrap().frames = i32::MAX,
            1 => m.line.as_mut().unwrap().from.x = i32::MAX,
            2 => m.resources.turn_frames[0] += 1,
            3 => m.actor.generation += 1,
            4 => m.radians = f32::NAN,
            5 => m.visual.visual_position[0] = f32::NAN,
            6 => m.visual.radians += 1.,
            7 => m.visual.velocity[0] += 1.,
            _ => unreachable!(),
        }
        assert!(
            snapshot::encode(&state, sim.content()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn deferred_target_deletion_preserves_valid_snapshot_until_route_failure() {
    let mut sim = runtime(true, false, Facing::EAST);
    tick(
        &mut sim,
        vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 100,
            position: LotPosition::new(104, 104, 1),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })],
    );
    let target = sim.state().entities[&ObjectId(2)].info.reference;
    tick(
        &mut sim,
        vec![AcceptedCommand::BeginRoute {
            entity: actor(),
            target: Some(target),
            goals: vec![RouteGoal::point(LotPosition::new(88, 24, 1))],
        }],
    );
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    assert!(
        matches!(sim.state().source_routing.routes.values().next(), Some(RouteMotionRecord::Active(m)) if m.line.is_some())
    );
    // Delete is deferred until after the route owner has already dispatched.
    tick(&mut sim, vec![AcceptedCommand::Delete { entity: target }]);
    let bytes = sim.snapshot().unwrap();
    sim.restore(&bytes).unwrap();
    assert!(
        matches!(sim.state().source_routing.routes.values().next(), Some(RouteMotionRecord::Active(m)) if m.line.is_none())
    );
    let events = tick(&mut sim, vec![]);
    assert!(events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::RouteFinished { success: false, .. })));
}

#[test]
fn external_direction_write_rebases_embedded_and_published_pose_atomically() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(88, 24, 1)));
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    let mut state = sim.state().clone();
    sim_core::runtime_memory::write_memory(
        &mut state,
        sim.content(),
        &MemoryAddress::Entity {
            entity: actor(),
            field: EntityField::ObjectData,
            index: 1,
        },
        i16::from(Facing::WEST.0),
    )
    .unwrap();
    let RouteMotionRecord::Active(m) = state.source_routing.routes.values().next().unwrap() else {
        panic!()
    };
    assert!(m.line.is_none());
    assert_eq!(m.phase, MotionPhase::Ready);
    assert_eq!(m.visual.facing, Facing::WEST);
    assert_eq!(&m.visual, &state.source_routing.avatars[&actor()]);
    let bytes = snapshot::encode(&state, sim.content()).unwrap();
    sim.restore(&bytes).unwrap();
    tick(&mut sim, vec![]);
}

#[test]
fn changed_style_downgrades_without_synthetic_frames_or_snapshot_failure() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(88, 24, 1)));
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    let events = tick(
        &mut sim,
        vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor(),
                field: EntityField::ObjectData,
                index: 17,
            },
            value: 1,
        }],
    );
    assert!(events.iter().any(|e| matches!(e, RuntimeEvent::RouteAnimationUnavailable { reason, .. } if reason.contains("context changed"))));
    assert!(matches!(
        sim.state().source_routing.routes.values().next(),
        Some(RouteMotionRecord::Unavailable { .. })
    ));
    sim.snapshot().unwrap();
}

#[test]
fn crossing_pool_context_downgrades_on_the_accepted_boundary_tick() {
    let mut lot = LotModel::new(8, 8, 1).unwrap();
    for x in 2..8 {
        for y in 0..8 {
            lot.set_floor(
                sim_core::world::TilePos::new(x, y, 1),
                sim_core::world::tiles::FLOOR_POOL,
            )
            .unwrap();
        }
    }
    let mut sim = runtime_on_lot(true, false, Facing::EAST, lot);
    begin(&mut sim, RouteGoal::point(LotPosition::new(56, 24, 1)));
    for _ in 0..200 {
        let events = tick(&mut sim, vec![]);
        if events.iter().any(|e| matches!(e, RuntimeEvent::RouteAnimationUnavailable { reason, .. } if reason.contains("context changed"))) {
            assert_eq!(sim.state().entities[&actor().object_id].info.position.x, 32);
            sim.snapshot().unwrap();
            return;
        }
    }
    panic!("pool boundary did not produce explicit source-context diagnostic");
}

#[test]
fn pool_route_uses_swim_half_index_and_source_double_duration() {
    let mut lot = LotModel::new(8, 8, 1).unwrap();
    for x in 0..8 {
        for y in 0..8 {
            lot.set_floor(
                sim_core::world::TilePos::new(x, y, 1),
                sim_core::world::tiles::FLOOR_POOL,
            )
            .unwrap();
        }
    }
    let mut sim = runtime_on_lot(true, true, Facing::EAST, lot);
    begin(&mut sim, RouteGoal::point(LotPosition::new(56, 24, 1)));
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    let RouteMotionRecord::Active(m) = sim.state().source_routing.routes.values().next().unwrap()
    else {
        panic!()
    };
    assert!(m.resources.in_pool);
    assert_eq!(m.resources.walk_style, 0);
    assert_eq!(m.resources.names[1], "source-vector-20.anim");
    assert_eq!(m.resources.names[2], "source-vector-21.anim");
    let line = m.line.as_ref().unwrap();
    let ordinary = SourceLine::new(line.from, line.to, line.last, &resources());
    assert_eq!(line.total_frames, ordinary.total_frames * 2);
}

#[test]
fn source_walk_reuses_preexisting_layers_by_first_two_names_and_keeps_carry() {
    use sim_core::avatars::timeline::AnimationState;
    let mut sim = runtime(true, false, Facing::EAST);
    let mut state = sim.state().clone();
    let mut layers: Vec<_> = [3, 20, 9]
        .into_iter()
        .map(|i| {
            AnimationState::new(
                sim.content()
                    .animation_by_resource(&format!("source-vector-{i}.anim"))
                    .unwrap()
                    .clone(),
                true,
            )
            .unwrap()
        })
        .collect();
    layers[0].speed = 0.5;
    layers[1].speed = 2.;
    layers[2].speed = 0.25;
    let carry = AnimationState::new(
        sim.content()
            .animation_by_resource("source-vector-25.anim")
            .unwrap()
            .clone(),
        false,
    )
    .unwrap();
    let avatar = state
        .entities
        .get_mut(&actor().object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.animations.animations = layers;
    avatar.animations.carry = Some(carry.clone());
    sim = SimRuntime::from_state(state, sim.content().clone(), RuntimeRole::Authority).unwrap();
    begin(&mut sim, RouteGoal::point(LotPosition::new(28, 24, 1)));
    for _ in 0..4 {
        tick(&mut sim, vec![]);
    }
    let RouteMotionRecord::Active(m) = sim.state().source_routing.routes.values().next().unwrap()
    else {
        panic!()
    };
    assert!(m.walk_reused);
    let set = &sim.state().entities[&actor().object_id]
        .avatar
        .as_ref()
        .unwrap()
        .animations;
    assert_eq!(set.animations[2].metadata.resource, "source-vector-9.anim");
    assert_eq!(set.animations[0].speed, 0.5);
    assert_eq!(set.animations[1].speed, 2.);
    assert!(set.animations.iter().all(|a| a.backwards));
    assert_eq!(set.carry.as_ref().unwrap().metadata, carry.metadata);
    sim.snapshot().unwrap();
}

#[test]
fn completed_route_retained_pose_rejects_teleport_and_yaw_tampering() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(30, 24, 1)));
    for _ in 0..200 {
        tick(&mut sim, vec![]);
        if sim.state().continuations.is_empty() {
            break;
        }
    }
    assert!(sim.state().continuations.is_empty());
    for case in 0..3 {
        let mut state = sim.state().clone();
        let pose = state.source_routing.avatars.get_mut(&actor()).unwrap();
        match case {
            0 => pose.radians += 1.,
            1 => pose.visual_position[0] += 2.,
            2 => pose.visual_position[2] += 1.,
            _ => unreachable!(),
        }
        assert!(
            snapshot::encode(&state, sim.content()).is_err(),
            "retained case {case}"
        );
    }
}

fn lock_actor_tile(sim: &mut SimRuntime) -> sim_core::world::build::DurableBuildRequest {
    use sim_core::world::build::{BuildAuthority, BuildEdit, BuildIntent};
    let position = sim.state().world.object(actor()).unwrap().position;
    let mut authority = BuildAuthority::new(actor(), PersistentId(42), 1000);
    authority.can_build = true;
    authority.prices.floor = 1;
    let intent = BuildIntent::new(
        99,
        actor(),
        sim.state().world.lot.revision().architecture,
        vec![BuildEdit::SetFloor {
            tile: position.tile().unwrap(),
            pattern: 1,
        }],
    );
    let preview = sim.preview_build(&intent, &authority).unwrap();
    tick(
        sim,
        vec![AcceptedCommand::BeginBuild { preview, authority }],
    );
    sim.state().world.builds.pending_effect().unwrap().clone()
}
fn unlock_actor_tile(sim: &mut SimRuntime, request: sim_core::world::build::DurableBuildRequest) {
    use sim_core::world::build::{DurableBuildStatus, ServerBuildConfirmation};
    tick(
        sim,
        vec![AcceptedCommand::CompleteBuild(ServerBuildConfirmation {
            operation: request.operation,
            actor: request.actor,
            owner: request.owner,
            preview_hash: request.preview_hash,
            charged_cost: request.cost,
            durable_receipt: 101,
            status: DurableBuildStatus::Committed,
            created_objects: Default::default(),
        })],
    );
}
#[test]
fn build_lock_defers_same_position_walk_start_without_changing_preview() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(28, 24, 1)));
    let request = lock_actor_tile(&mut sim);
    let object = sim.state().world.object(actor()).unwrap().clone();
    let route = sim.state().continuations.clone();
    for _ in 0..3 {
        tick(&mut sim, vec![]);
    }
    assert_eq!(sim.state().world.object(actor()).unwrap(), &object);
    assert_eq!(sim.state().continuations, route);
    assert_eq!(sim.state().world.builds.pending_effect(), Some(&request));
    sim.snapshot().unwrap();
    unlock_actor_tile(&mut sim, request);
    assert!(sim.state().world.builds.pending_effect().is_none());
    for _ in 0..100 {
        tick(&mut sim, vec![]);
        if sim.state().continuations.is_empty() {
            return;
        }
    }
    panic!("source route did not resume after build unlock");
}
#[test]
fn build_lock_defers_final_frame_and_cleanup_until_preview_commits() {
    let mut sim = runtime(true, false, Facing::EAST);
    begin(&mut sim, RouteGoal::point(LotPosition::new(28, 24, 1)));
    loop {
        tick(&mut sim, vec![]);
        if matches!(sim.state().source_routing.routes.values().next(), Some(RouteMotionRecord::Active(m))
            if m.line.as_ref().is_some_and(|l| l.last && l.frames == l.total_frames - 1))
        {
            break;
        }
    }
    let request = lock_actor_tile(&mut sim);
    let object = sim.state().world.object(actor()).unwrap().clone();
    for _ in 0..3 {
        tick(&mut sim, vec![]);
    }
    assert_eq!(sim.state().world.object(actor()).unwrap(), &object);
    assert_eq!(sim.state().world.builds.pending_effect(), Some(&request));
    assert!(!sim.state().continuations.is_empty());
    unlock_actor_tile(&mut sim, request);
    for _ in 0..100 {
        tick(&mut sim, vec![]);
        if sim.state().continuations.is_empty() {
            assert_eq!(
                sim.state().world.object(actor()).unwrap().motion,
                sim_core::world::footprints::ObstacleMotion::StationaryAvatar
            );
            return;
        }
    }
    panic!("source final frame did not resume after build unlock");
}
