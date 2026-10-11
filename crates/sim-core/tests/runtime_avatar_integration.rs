use sim_core::{
    avatars::{
        outfits::OutfitReference,
        timeline::{AnimationMetadata, AnimationState},
    },
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
    runtime_avatars::{
        animate, apply_person_outfit, initialize_walk_animations, resolve_person_outfit,
    },
    state::{AnimationKey, ContentSet, LegacyJobUniform, ObjectDefinition, SimState, TuningSet},
    vm::*,
    world::{Facing, LotModel, TilePos},
};

fn metadata(name: &str) -> AnimationMetadata {
    AnimationMetadata {
        resource: format!("{name}.anim"),
        num_frames: 4,
        time_properties: vec![sim_core::avatars::events::TimeProperty::xevt(0, 8)],
    }
}
fn strings(name: &str) -> Vec<String> {
    vec![String::new(), name.into()]
}
fn source_content() -> ContentSet {
    let mut avatar = ObjectDefinition::new(100, 4);
    avatar.slot_count = 1;
    avatar.body_string_id = 200;
    let mut owner = ObjectDefinition::new(200, 0);
    owner.animation_table_id = 500;
    let mut stack = ObjectDefinition::new(300, 0);
    stack.animation_table_id = 600;
    let names = [
        "owner-adult",
        "owner-child",
        "stack-adult",
        "stack-child",
        "global",
        "stock",
        "misc-adult",
        "misc-child",
        "fallback-adult",
        "fallback-child",
        "walk-stand",
        "walk-sit",
        "walk-kneel",
        "child-stand",
        "special-sit",
        "a2o-rarm-carry-loop",
    ];
    let records = names
        .iter()
        .enumerate()
        .map(|(id, name)|
        // Ingestion identity intentionally does not resemble the executable
        // STR owner/scope/ID. Resource names perform runtime lookup.
        (AnimationKey { owner: 999, scope: 777, id: id as u16 }, metadata(name)))
        .collect();
    let mut body = vec![String::new(); 35];
    body[1] = "b001mfit_01,BODY=b001mfitlgt_01".into();
    body[14] = "LGT".into();
    let expression = |index, operator, scope, value, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs: Variable::new(Scope::MyObjectAttributes, index),
                rhs: Variable::new(scope, value),
                is_signed: 0,
                operator,
            }
            .encode(),
        )
    };
    let routine = RoutineKey {
        scope: RoutineScope::Private(200),
        id: 4096,
    };
    let mut routines = RoutineStore::new();
    routines
        .insert(
            routine,
            VmRoutine::new(
                4096,
                1,
                4,
                vec![
                    VmInstruction::new(44, 3, 1, [1, 0, 0, 0, 0, 32, 2, 0]),
                    expression(0, 3, Scope::Local, 0, 2),
                    expression(1, 3, Scope::Literal, 1, 0),
                    expression(2, 3, Scope::Literal, 1, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    ContentSet::new(
        routines,
        vec![avatar, owner, stack],
        records,
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![
        ((200, 500), strings("owner-adult")),
        ((200, 501), strings("owner-child")),
        ((300, 600), strings("stack-adult")),
        ((300, 601), strings("stack-child")),
        ((200, 129), strings("fallback-adult")),
        ((200, 130), strings("fallback-child")),
        ((0, 128), strings("global")),
        ((0, 130), strings("stock")),
        ((0, 156), strings("misc-adult")),
        ((0, 157), strings("misc-child")),
        (
            (0, 150),
            vec![
                String::new(),
                "walk-sit".into(),
                "walk-kneel".into(),
                "walk-stand".into(),
            ],
        ),
        (
            (0, 151),
            vec![
                String::new(),
                "walk-sit".into(),
                "walk-kneel".into(),
                "child-stand".into(),
            ],
        ),
        ((0, 158), vec!["adult-swim".into()]),
        ((0, 160), vec!["child-swim".into()]),
        ((100, 150), vec![String::new(), "special-sit".into()]),
        ((100, 200), body),
    ])
    .unwrap()
    .with_suits(vec![(
        (100, 1, 0),
        ResolvedSuit::Reference(OutfitReference::Legacy {
            definition: "b001mfit_01,BODY=b001mfitlgt_01".into(),
            head: false,
            hand_group: Some("body-hands".into()),
        }),
    )])
    .unwrap()
    .with_legacy_job_uniforms(vec![
        (
            (4, 2),
            LegacyJobUniform {
                male_mesh: "J$G$B_01".into(),
                female_mesh: Some("F$G$B_01".into()),
                texture: "T$G$B$C_01".into(),
            },
        ),
        (
            (4, 3),
            LegacyJobUniform {
                male_mesh: String::new(),
                female_mesh: Some("ignored-female".into()),
                texture: "ignored".into(),
            },
        ),
    ])
    .unwrap()
}
fn runtime(content: ContentSet) -> SimRuntime {
    SimRuntime::new(
        content,
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap()
}
fn spawned(content: ContentSet) -> SimRuntime {
    let mut runtime = runtime(content);
    let commands = [(100, 3, true), (300, 7, false)]
        .into_iter()
        .map(|(guid, x, avatar)| {
            AcceptedCommand::Spawn(SpawnSpec {
                guid,
                position: TilePos::new(x, 3, 1).center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(if avatar { 42 } else { 0 }),
                avatar,
            })
        })
        .collect();
    let tick = runtime.next_tick(commands).unwrap();
    runtime.step(&tick).unwrap();
    runtime
}
fn state() -> (ContentSet, SimState, EntityRef) {
    let content = source_content();
    let runtime = spawned(content.clone());
    let entity = runtime.state().entities[&ObjectId(1)].info.reference;
    (content, runtime.state().clone(), entity)
}
fn request(state: &SimState, entity: EntityRef, scope: u32) -> AnimationRequest {
    let stack = state.entities[&ObjectId(2)].info.reference;
    AnimationRequest {
        context: FrameContext {
            caller: entity,
            callee: entity,
            code_owner: 200,
            stack_object: stack.object_id,
            stack_object_ref: Some(stack),
        },
        animation_id: 1,
        scope,
        mode: 1,
        backwards: false,
        hurryable: false,
        expected_events: 2,
        event_local: 0,
        store_event_in_parameter: false,
    }
}
fn current_resource(state: &SimState, entity: EntityRef) -> &str {
    &state.entities[&entity.object_id]
        .avatar
        .as_ref()
        .unwrap()
        .animations
        .animations[0]
        .metadata
        .resource
}

#[test]
fn animation_scopes_resolve_source_owner_and_exact_child_tables() {
    let (content, mut state, entity) = state();
    for (age, expected) in [
        (
            0,
            [
                "owner-child",
                "global",
                "stock",
                "misc-child",
                "stack-child",
            ],
        ),
        (
            18,
            [
                "owner-adult",
                "global",
                "stock",
                "misc-adult",
                "stack-adult",
            ],
        ),
    ] {
        state
            .entities
            .get_mut(&entity.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .person_data
            .values[58] = age;
        for (scope, name) in [0, 1, 2, 3, 65_536].into_iter().zip(expected) {
            let request = request(&state, entity, scope);
            assert_eq!(
                animate(&mut state, &content, &request).unwrap(),
                HostResponse::Complete(PrimitiveExit::GotoTrue)
            );
            assert_eq!(current_resource(&state, entity), format!("{name}.anim"));
        }
    }
}

#[test]
fn source_animation_provider_folds_ascii_case_without_trimming_str_names() {
    let build = |name| {
        ContentSet::new(
            RoutineStore::new(),
            vec![],
            vec![(
                AnimationKey {
                    owner: 0,
                    scope: 0,
                    id: 0,
                },
                metadata("MiXeD"),
            )],
            TuningSet::default(),
        )
        .unwrap()
        .with_strings(vec![((0, 128), strings(name))])
        .unwrap()
    };
    let (_, mut state, entity) = state();
    let request = request(&state, entity, 1);
    animate(&mut state, &build("mIxEd"), &request).unwrap();
    assert_eq!(current_resource(&state, entity), "MiXeD.anim");
    let before = state.clone();
    assert_eq!(
        animate(&mut state, &build(" MiXeD "), &request).unwrap(),
        HostResponse::Complete(PrimitiveExit::GotoTrueNextTick)
    );
    assert_eq!(state, before);
}

#[test]
fn configured_animation_table_falls_back_only_when_whole_table_absent() {
    let mut owner = ObjectDefinition::new(200, 0);
    owner.animation_table_id = 700;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![owner],
        vec![(
            AnimationKey {
                owner: 0,
                scope: 0,
                id: 0,
            },
            metadata("fallback-child"),
        )],
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![((200, 130), strings("fallback-child"))])
    .unwrap();
    let (_, mut state, entity) = state();
    let request = request(&state, entity, 0);
    assert_eq!(
        animate(&mut state, &content, &request).unwrap(),
        HostResponse::Complete(PrimitiveExit::GotoTrue)
    );
    assert_eq!(current_resource(&state, entity), "fallback-child.anim");
    let present = content
        .with_strings(vec![((200, 701), vec![String::new()])])
        .unwrap();
    let before = state.clone();
    assert_eq!(
        animate(&mut state, &present, &request).unwrap(),
        HostResponse::Complete(PrimitiveExit::GotoTrueNextTick)
    );
    assert_eq!(state, before);
}

#[test]
fn child_animation_table_addition_preserves_source_u16_wrap() {
    let mut owner = ObjectDefinition::new(200, 0);
    owner.animation_table_id = u16::MAX;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![owner],
        vec![(
            AnimationKey {
                owner: 1,
                scope: 4,
                id: 2,
            },
            metadata("wrapped"),
        )],
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![((200, 0), strings("wrapped"))])
    .unwrap();
    let (_, mut state, entity) = state();
    let request = request(&state, entity, 0);
    animate(&mut state, &content, &request).unwrap();
    assert_eq!(current_resource(&state, entity), "wrapped.anim");
}

#[test]
fn walk_initialization_preserves_empty_overrides_and_is_atomic_at_bounds() {
    let (content, mut state, entity) = state();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    initialize_walk_animations(avatar, 100, &content).unwrap();
    assert_eq!(avatar.walk_animations[1], "special-sit");
    assert_eq!(avatar.walk_animations[2], "walk-kneel");
    assert_eq!(avatar.walk_animations[3], "child-stand");
    assert_eq!(avatar.swim_animations[0], "child-swim");
    let invalid = ContentSet::new(RoutineStore::new(), vec![], vec![], TuningSet::default())
        .unwrap()
        .with_strings(vec![((0, 151), vec!["x".into(); 51])])
        .unwrap();
    let before = avatar.clone();
    assert!(initialize_walk_animations(avatar, 100, &invalid).is_err());
    assert_eq!(avatar, &before);
}

#[test]
fn reset_uses_posture_walk_name_and_source_named_default_carry() {
    let (content, mut state, entity) = state();
    let target = state.entities[&ObjectId(2)].info.reference;
    let mut request = request(&state, entity, 99);
    request.animation_id = 0;
    request.backwards = true;
    request.hurryable = true;
    let item = state.entities.get_mut(&entity.object_id).unwrap();
    item.slots[0] = Some(target);
    item.object_data[17] = 1;
    item.avatar.as_mut().unwrap().person_data.values[42] = 1;
    for (posture, name) in [
        (1, "special-sit"),
        (2, "walk-kneel"),
        (0, "child-stand"),
        (7, "child-stand"),
    ] {
        state
            .entities
            .get_mut(&entity.object_id)
            .unwrap()
            .avatar
            .as_mut()
            .unwrap()
            .person_data
            .values[0] = posture;
        assert_eq!(
            animate(&mut state, &content, &request).unwrap(),
            HostResponse::Complete(PrimitiveExit::GotoTrue)
        );
        let avatar = state.entities[&entity.object_id].avatar.as_ref().unwrap();
        assert_eq!(avatar.person_data.values[42], 4);
        assert_eq!(
            avatar.animations.animations[0].metadata.resource,
            format!("{name}.anim")
        );
        assert!(
            avatar.animations.animations[0].looping && avatar.animations.animations[0].backwards
        );
        assert_eq!(
            avatar.animations.animations[0].speed.to_bits(),
            1.2f32.to_bits()
        ); // reset ignores Hurryable
        assert_eq!(
            avatar.animations.carry.as_ref().unwrap().metadata.resource,
            "a2o-rarm-carry-loop.anim"
        );
    }
}

#[test]
fn reset_missing_resource_and_mode_three_have_distinct_early_side_effects() {
    let (content, mut state, entity) = state();
    let mut request = request(&state, entity, 0);
    animate(&mut state, &content, &request).unwrap();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[42] = 1;
    avatar.walk_animations[3] = "absent".into();
    avatar.animations.carry = Some(AnimationState::new(metadata("old-carry"), false).unwrap());
    request.animation_id = 0;
    animate(&mut state, &content, &request).unwrap();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    assert_eq!(avatar.person_data.values[42], 4);
    assert!(avatar.animations.animations.is_empty());
    assert!(avatar.animations.carry.is_some()); // source returns before carry handling
    avatar.person_data.values[42] = 1;
    avatar
        .animations
        .animations
        .push(AnimationState::new(metadata("old"), false).unwrap());
    request.mode = 3;
    animate(&mut state, &content, &request).unwrap();
    let avatar = state.entities[&entity.object_id].avatar.as_ref().unwrap();
    assert_eq!(avatar.person_data.values[42], 1);
    assert_eq!(avatar.animations.animations.len(), 1);
    assert!(avatar.animations.carry.is_none());
}

#[test]
fn normal_missing_resource_yields_without_clearing_mode_three_carry() {
    let (content, mut state, entity) = state();
    let mut request = request(&state, entity, 1);
    request.mode = 3;
    request.animation_id = 999;
    state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .animations
        .carry = Some(AnimationState::new(metadata("carry"), false).unwrap());
    let before = state.clone();
    assert_eq!(
        animate(&mut state, &content, &request).unwrap(),
        HostResponse::Complete(PrimitiveExit::GotoTrueNextTick)
    );
    assert_eq!(state, before);
}

#[test]
fn missing_default_carry_rejects_reset_without_partial_avatar_mutation() {
    let (_, mut state, entity) = state();
    let target = state.entities[&ObjectId(2)].info.reference;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![],
        vec![(
            AnimationKey {
                owner: 0,
                scope: 0,
                id: 0,
            },
            metadata("child-stand"),
        )],
        TuningSet::default(),
    )
    .unwrap();
    let mut request = request(&state, entity, 0);
    request.animation_id = 0;
    let item = state.entities.get_mut(&entity.object_id).unwrap();
    item.slots[0] = Some(target);
    item.avatar.as_mut().unwrap().person_data.values[42] = 1;
    let before = state.clone();
    assert!(animate(&mut state, &content, &request).is_err());
    assert_eq!(state, before);
}

#[test]
fn generated_outfit_and_missing_job_provider_fail_before_body_commit() {
    let (content, mut state, entity) = state();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[56] = 4;
    avatar.person_data.values[57] = 99;
    let before = state.clone();
    assert!(apply_person_outfit(&mut state, &content, entity, 3).is_err());
    assert_eq!(state, before);
    let mut object = ObjectDefinition::new(100, 0);
    object.body_string_id = 200;
    let mut body = vec![String::new(); 15];
    body[1] = "x".repeat(16_385);
    body[14] = "lgt".into();
    let oversized = ContentSet::new(
        RoutineStore::new(),
        vec![object],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![((100, 200), body)])
    .unwrap();
    assert!(apply_person_outfit(&mut state, &oversized, entity, 0).is_err());
    assert_eq!(state, before);
}

#[test]
fn source_resource_aliases_cannot_select_conflicting_animation_metadata() {
    let a = metadata("same");
    let mut b = a.clone();
    b.resource = "SAME.ANIM".into();
    b.num_frames += 1;
    let records = |second| {
        vec![
            (
                AnimationKey {
                    owner: 100,
                    scope: 0,
                    id: 1,
                },
                a.clone(),
            ),
            (
                AnimationKey {
                    owner: 200,
                    scope: 1,
                    id: 2,
                },
                second,
            ),
        ]
    };
    assert!(ContentSet::new(
        RoutineStore::new(),
        vec![],
        records(b),
        TuningSet::default()
    )
    .is_err());
    let mut alias = a.clone();
    alias.resource = "Same.anim".into();
    let identical = ContentSet::new(
        RoutineStore::new(),
        vec![],
        records(alias),
        TuningSet::default(),
    )
    .unwrap();
    assert_eq!(
        identical
            .animation_by_resource("SAME.anim")
            .unwrap()
            .resource,
        a.resource
    );
}

#[test]
fn ts1_outfits_read_live_gender_age_job_and_correct_hand_group_source() {
    let (content, mut state, entity) = state();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[58] = 18;
    avatar.outfits.defaults.daywear = OutfitReference::Legacy {
        definition: "stored".into(),
        head: false,
        hand_group: Some("stored-hands".into()),
    };
    assert_eq!(
        resolve_person_outfit(&state, &content, entity, 1).unwrap(),
        Some(OutfitReference::Legacy {
            definition: "nmfit_01,BODY=nmfitlgt_01".into(),
            head: false,
            hand_group: Some("stored-hands".into())
        })
    );
    assert!(
        matches!(resolve_person_outfit(&state, &content, entity, 0).unwrap(), Some(OutfitReference::Legacy { hand_group: Some(value), .. }) if value == "body-hands")
    );
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[58] = 17;
    avatar.person_data.values[65] = 1;
    avatar.person_data.values[56] = 4;
    avatar.person_data.values[57] = 2;
    assert_eq!(
        resolve_person_outfit(&state, &content, entity, 3).unwrap(),
        Some(OutfitReference::Legacy {
            definition: "fuchd_01,BODY=tuchdlgt_01".into(),
            head: false,
            hand_group: Some("stored-hands".into())
        })
    );
    state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .person_data
        .values[57] = 3;
    assert!(
        matches!(resolve_person_outfit(&state, &content, entity, 3).unwrap(), Some(OutfitReference::Legacy { hand_group: Some(value), .. }) if value == "body-hands")
    );
    apply_person_outfit(&mut state, &content, entity, 1).unwrap();
    assert!(apply_person_outfit(&mut state, &content, entity, 128).unwrap());
    assert!(state.entities[&entity.object_id]
        .avatar
        .as_ref()
        .unwrap()
        .outfits
        .body
        .is_none());
}

#[test]
fn female_job_uniform_empty_mesh_does_not_repeat_the_male_fallback() {
    let (content, mut state, entity) = state();
    let content = content
        .with_legacy_job_uniforms(vec![(
            (4, 4),
            LegacyJobUniform {
                male_mesh: "male-mesh".into(),
                female_mesh: Some(String::new()),
                texture: "T$G$B$C_01".into(),
            },
        )])
        .unwrap();
    let avatar = state
        .entities
        .get_mut(&entity.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[58] = 18;
    avatar.person_data.values[65] = 1;
    avatar.person_data.values[56] = 4;
    avatar.person_data.values[57] = 4;
    avatar.outfits.defaults.daywear = OutfitReference::Legacy {
        definition: "stored".into(),
        head: false,
        hand_group: Some("stored-hands".into()),
    };
    assert_eq!(
        resolve_person_outfit(&state, &content, entity, 3).unwrap(),
        Some(OutfitReference::Legacy {
            definition: ",BODY=tffitlgt_01".into(),
            head: false,
            hand_group: Some("stored-hands".into()),
        })
    );
}

#[test]
fn actual_bhav_animation_event_branches_continue_after_runtime_snapshot() {
    let content = source_content();
    let mut a = spawned(content.clone());
    let entity = a.state().entities[&ObjectId(1)].info.reference;
    let context = request(a.state(), entity, 0).context;
    let input = a
        .next_tick(vec![
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity,
                    field: EntityField::PersonData,
                    index: 58,
                },
                value: 18,
            },
            AcceptedCommand::StartBehavior {
                entity,
                routine: RoutineKey {
                    scope: RoutineScope::Private(200),
                    id: 4096,
                },
                context,
                args: vec![0; 4],
                replace: true,
            },
        ])
        .unwrap();
    a.step(&input).unwrap();
    let bytes = a.snapshot().unwrap();
    let mut b = runtime(content);
    b.restore(&bytes).unwrap();
    for _ in 0..9 {
        let input = a.next_tick(vec![]).unwrap();
        assert_eq!(
            a.step(&input).unwrap().state_hash,
            b.step(&input).unwrap().state_hash
        );
    }
    assert_eq!(
        a.state().entities[&entity.object_id].attributes,
        [9, 2, 1, 0]
    );
}

#[test]
fn current_outfit_memory_signal_applies_body_during_the_accepted_tick() {
    let content = source_content();
    let mut runtime = spawned(content);
    let entity = runtime.state().entities[&ObjectId(1)].info.reference;
    let tick = runtime
        .next_tick(vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity,
                field: EntityField::PersonData,
                index: 8,
            },
            value: 1,
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    let avatar = runtime.state().entities[&entity.object_id]
        .avatar
        .as_ref()
        .unwrap();
    assert_eq!(avatar.person_data.values[8], 1);
    // Outfit child logic intentionally treats age zero as adult; animation
    // lookup's child condition treats it as child, matching the two C# helpers.
    assert!(
        matches!(&avatar.outfits.body, Some(OutfitReference::Legacy { definition, hand_group: Some(hand), .. })
        if definition == "nmfit_01,BODY=nmfitlgt_01" && hand == "body-hands")
    );
}
