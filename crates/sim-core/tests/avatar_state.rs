use sim_core::avatars::motives::Motive;
use sim_core::avatars::outfits::{DefaultSuits, LegacySuitInputs, OutfitReference};
use sim_core::avatars::skills::{set_skill_lock, Skill, SkillLockResult};
use sim_core::avatars::state::{AvatarPermissions, PersonData};
use sim_core::avatars::{AvatarPlatform, AvatarState, AvatarTickContext};
use sim_core::ids::{EntityRef, ObjectId, PersistentId};

fn avatar() -> AvatarState {
    AvatarState::new(
        EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        },
        PersistentId(42),
        AvatarPlatform::Ts1,
    )
}

#[test]
fn avatar_source_defaults_person_widths_and_skill_policy() {
    let mut a = avatar();
    a.platform = AvatarPlatform::Tso;
    assert_eq!(a.person_data.values.len(), 101);
    assert_eq!(a.read_person_data(7).unwrap(), 1000);
    assert!(a.read_person_data(101).is_err());
    a.skill_mode = 1;
    a.write_person_data(10, 500).unwrap();
    assert_eq!(a.read_person_data(10).unwrap(), 0);
    assert_eq!(a.read_person_data(70).unwrap(), 0x7fff);
    a.permissions = AvatarPermissions::Roommate;
    a.write_person_data(10, 500).unwrap();
    assert_eq!(a.read_person_data(10).unwrap(), 500);
    a.avatar_flags = 2;
    a.lot_category = 7;
    a.skill_mode = 2;
    a.write_person_data(10, 501).unwrap();
    assert_eq!(a.read_person_data(10).unwrap(), 502);
    a.write_person_data(10, 400).unwrap();
    assert_eq!(a.read_person_data(10).unwrap(), 400);
    a.persistent_id = PersistentId(0);
    a.lot_category = 0;
    a.write_person_data(10, 401).unwrap();
    assert_eq!(a.read_person_data(10).unwrap(), 401);
}

#[test]
fn avatar_skill_lock_budget_and_legacy_negative_level_are_preserved() {
    let mut a = avatar();
    a.set_skill_locks(8);
    a.person_data.values[17] = 900;
    a.person_data.values[12] = 900;
    a.person_data.values[81] = 200;
    a.person_data.values[82] = 300;
    assert_eq!(
        set_skill_lock(&mut a.person_data, 255, 9),
        SkillLockResult::Applied {
            skill: Skill::Mechanical,
            level: 3
        }
    );
    assert_eq!(a.person_data.values[86], 300);
    assert_eq!(
        set_skill_lock(&mut a.person_data, 0, -1),
        SkillLockResult::Applied {
            skill: Skill::Body,
            level: -1
        }
    );
    assert_eq!(a.person_data.values[81], -100);
    a.set_skill_locks(0);
    assert_eq!(
        set_skill_lock(&mut a.person_data, 0, 0),
        SkillLockResult::NoCapacity
    );
}

#[test]
fn avatar_outfit_resolution_storage_and_persistence() {
    let mut a = avatar();
    a.platform = AvatarPlatform::Tso;
    a.outfits.defaults = DefaultSuits::new(true);
    a.person_data.values[65] = 1;
    a.write_person_data(8, 2).unwrap();
    assert_eq!(a.outfits.body, Some(OutfitReference::Id(0x620000000d)));
    a.outfits.set_stored(2, 123);
    assert_eq!(a.outfits.body, Some(OutfitReference::Id(0x620000000d)));
    a.write_person_data(91, 4).unwrap();
    a.write_person_data(92, 7).unwrap();
    a.write_person_data(8, 3).unwrap();
    assert_eq!(a.outfits.body, Some(OutfitReference::Id(0x5830000000d)));
    a.write_person_data(8, 1).unwrap();
    a.motives.set(Motive::SleepState, -1);
    let persisted = a.persistent_state().unwrap();
    assert_eq!(persisted.body, Some(a.outfits.defaults.daywear.clone()));
    assert_eq!(persisted.motive_data[11], 0);
    assert_eq!(a.motives.get(Motive::SleepState), -1);
    assert!(OutfitReference::parse_source("body_name", AvatarPlatform::Ts1).is_err());
    assert_eq!(
        OutfitReference::parse_source("0xABC", AvatarPlatform::Ts1).unwrap(),
        OutfitReference::Id(0xabc)
    );
}

#[test]
fn avatar_legacy_suits_preserve_child_age_zero_distinction() {
    let mut strings = vec![String::new(); 35];
    strings[1] = "skinny_skn".into();
    strings[14] = "LGT".into();
    let mut input = LegacySuitInputs {
        body_strings: strings,
        gender: 0,
        age: 10,
        hand_group: Some("hands".into()),
        job_uniform: None,
    };
    assert_eq!(
        input.resolve(1).unwrap(),
        OutfitReference::Legacy {
            definition: "nuchd_01,BODY=nuchdlgt_01".into(),
            head: false,
            hand_group: Some("hands".into())
        }
    );
    input.age = 0;
    assert_eq!(
        input.resolve(1).unwrap(),
        OutfitReference::Legacy {
            definition: "nmskn_01,BODY=nmsknlgt_01".into(),
            head: false,
            hand_group: Some("hands".into())
        }
    );
}

#[test]
fn avatar_custom_pet_load_keeps_gender_and_restores_raw_motive_overfill() {
    let mut source = avatar();
    source.set_skill_locks(8);
    source.person_data.values[10] = 900;
    let mut persisted = source.persistent_state().unwrap();
    persisted.custom_guid = 0x1234;
    persisted.motive_data[7] = 250;
    let mut target = avatar();
    target.person_data.values[65] = 16;
    target.apply_persistent_state(&persisted).unwrap();
    assert!(target.is_cat());
    assert_eq!(target.motives.get(Motive::Hunger), 250);
    assert_eq!(target.person_data.values[10], 900);
    assert_eq!(target.skill_locks(), 8);
    let mut restored: AvatarState =
        bincode::deserialize(&bincode::serialize(&target).unwrap()).unwrap();
    let context = AvatarTickContext {
        minute: 2,
        ..Default::default()
    };
    for _ in 0..30 {
        assert_eq!(target.tick(&context), restored.tick(&context));
    }
    assert_eq!(target, restored);
    restored.validate().unwrap();
}

#[test]
fn avatar_message_timeout_runs_while_paused_and_tick_counter_wraps() {
    let mut a = avatar();
    a.set_message("😀".into()).unwrap();
    assert_eq!(a.message_timeout, 151);
    let context = AvatarTickContext {
        thread_paused: true,
        ..Default::default()
    };
    for _ in 0..151 {
        a.tick(&context).unwrap();
    }
    assert!(a.message.is_empty());
    assert_eq!(a.person_data.values[27], 0);
    a.person_data.values[27] = i16::MAX;
    a.tick(&AvatarTickContext::default()).unwrap();
    assert_eq!(a.person_data.values[27], i16::MIN);
}

#[test]
fn avatar_neighbor_inheritance_preserves_cat_service_type_and_schedule() {
    let mut a = avatar();
    a.person_data.values[65] = 16;
    a.person_data.values[32] = 254;
    a.person_data.values[35] = 9;
    let mut neighbor = PersonData::default();
    neighbor.values[61] = 10;
    neighbor.values[34] = 3;
    a.inherit_neighbor(neighbor, 11, Some(10)).unwrap();
    assert!(a.is_cat());
    assert_eq!(a.person_data.values[32], 254);
    assert_eq!(a.person_data.values[35], 9);
    assert_eq!(a.person_data.values[31], 11);
    assert_eq!(a.person_data.values[34], 0);
}

#[test]
fn avatar_head_seek_timeout_and_generation_target_survive_snapshot() {
    let mut a = avatar();
    a.write_person_data(41, 2).unwrap();
    a.write_person_data(42, 1).unwrap();
    a.write_person_data(43, 1).unwrap();
    a.write_person_data(45, 3).unwrap();
    a.resolve_head_seek_target(Some(EntityRef {
        object_id: ObjectId(2),
        generation: 1,
    }))
    .unwrap();
    for _ in 0..3 {
        a.tick(&AvatarTickContext::default()).unwrap();
    }
    assert_eq!(a.head_seek.weight, 3.0);
    assert_eq!(a.read_person_data(42).unwrap(), 4);
    let mut restored: AvatarState = bincode::deserialize(&bincode::serialize(&a).unwrap()).unwrap();
    for _ in 0..3 {
        assert_eq!(
            a.tick(&AvatarTickContext::default()),
            restored.tick(&AvatarTickContext::default())
        );
    }
    assert_eq!(a.read_person_data(42).unwrap(), 8);
    assert_eq!(a.read_person_data(43).unwrap(), 0);
    assert_eq!(a, restored);
}

#[test]
fn avatar_motive_primitive_clear_ignores_bad_index_and_rate_scales_before_narrowing() {
    let mut a = avatar();
    assert_eq!(a.apply_motive_change(255, 0, 0, true, false), Ok(true));
    a.motives.set(Motive::Energy, 0);
    a.apply_motive_change(Motive::Energy as u8, 1600, 100, false, true)
        .unwrap();
    // TS1 positive energy scales to 35200, then short wraps to -30336 before
    // Once's sign-dependent maximum test, which skips an already-lower motive.
    assert_eq!(a.motives.get(Motive::Energy), 0);
    a.apply_motive_change(Motive::Energy as u8, 1, 100, false, true)
        .unwrap();
    assert_eq!(a.motives.get(Motive::Energy), 22);
}

#[test]
fn avatar_animation_reset_ends_head_seek_even_when_resource_is_absent() {
    use sim_core::avatars::timeline::{AnimationCommand, AnimationMode, AnimationResult};
    let mut a = avatar();
    a.write_person_data(42, 1).unwrap();
    let mut command = AnimationCommand::play("absent.anim");
    command.reset = true;
    assert_eq!(
        a.apply_animation(&command, None).unwrap(),
        AnimationResult::Complete
    );
    assert_eq!(a.read_person_data(42).unwrap(), 4);
    a.write_person_data(42, 1).unwrap();
    command.mode = AnimationMode::ClearCarryAndWait;
    assert_eq!(
        a.apply_animation(&command, None).unwrap(),
        AnimationResult::Complete
    );
    assert_eq!(a.read_person_data(42).unwrap(), 1);
}

#[test]
fn avatar_change_suit_keeps_original_operand_and_exact_accessory_names() {
    let mut a = avatar();
    a.person_data.values[8] = 5;
    a.set_default_daywear(OutfitReference::Id(42)).unwrap();
    assert_eq!(a.person_data.values[8], 5);
    assert_eq!(a.outfits.defaults.daywear, OutfitReference::Id(42));
    assert_eq!(a.outfits.body, Some(OutfitReference::Id(42)));
    a.set_body_outfit(OutfitReference::Id(99), 22).unwrap();
    assert_eq!(a.person_data.values[8], 22);
    assert_eq!(a.outfits.body, Some(OutfitReference::Id(99)));
    a.set_accessory_name("hat.apr".into(), false).unwrap();
    a.set_accessory_name("hat".into(), false).unwrap();
    a.set_accessory_name("hat.apr".into(), true).unwrap();
    assert_eq!(
        a.animations
            .bound_appearances
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["hat".to_owned()]
    );
    let before = a.clone();
    assert!(a.set_accessory_name("x".repeat(16_385), false).is_err());
    assert_eq!(a, before);
}

#[test]
fn avatar_persistent_worker_job_status_is_applied_after_job_map_load() {
    use sim_core::avatars::state::JobInfo;
    let mut source = avatar();
    source.write_person_data(91, 2).unwrap();
    let mut persisted = source.persistent_state().unwrap();
    persisted.is_worker = true;
    persisted.job_info.insert(
        2,
        JobInfo {
            experience: 12,
            level: 3,
            sick_days: 4,
            status_flags: 0,
        },
    );
    let mut target = avatar();
    target.apply_persistent_state(&persisted).unwrap();
    assert_eq!(target.read_person_data(91).unwrap(), 2);
    assert_eq!(target.read_person_data(92).unwrap(), 3);
    assert_eq!(target.read_person_data(93).unwrap(), 12);
    assert_eq!(target.read_person_data(94).unwrap(), 4);
    assert_eq!(target.read_person_data(98).unwrap(), 1);
}
