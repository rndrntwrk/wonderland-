use sim_core::avatars::motives::{DecayContext, Motive, MotiveDecay, MotiveState, TsoMotiveTuning};
use sim_core::avatars::AvatarPlatform;

#[test]
fn avatar_motives_have_source_indices_defaults_and_overfill_clamp() {
    let mut state = MotiveState::default();
    assert_eq!(Motive::Energy as usize, 5);
    assert_eq!(Motive::Hunger as usize, 7);
    assert_eq!(Motive::SleepState as usize, 11);
    assert_eq!(state.get(Motive::SleepState), 0);
    assert_eq!(state.get(Motive::Hunger), 100);
    state.values[7] = 150;
    state.set(Motive::Hunger, 160);
    assert_eq!(state.get(Motive::Hunger), 150);
    state.set(Motive::Hunger, 140);
    assert_eq!(state.get(Motive::Hunger), 140);
    state.set(Motive::Hunger, -101);
    assert_eq!(state.get(Motive::Hunger), -100);
}

#[test]
fn avatar_motive_restore_gate_fraction_and_cap_survive_snapshot() {
    let mut state = MotiveState::default();
    state.set(Motive::Hunger, 0);
    assert!(!state.set_change(Motive::Hunger, 9000, 2)); // not ticked yet
    state.tick_changes(AvatarPlatform::Tso);
    assert!(state.set_change(Motive::Hunger, 9000, 2));
    assert!(!state.set_change(Motive::Hunger, -9000, -50));
    state.tick_changes(AvatarPlatform::Tso);
    assert_eq!(state.get(Motive::Hunger), 1);
    state.tick_changes(AvatarPlatform::Tso);
    assert_eq!(state.get(Motive::Hunger), 2);
    assert!(state.has_change(Motive::Hunger));
    state.set(Motive::Hunger, 3);
    state.tick_changes(AvatarPlatform::Tso); // retains whole accumulated fraction while already over cap
    assert_eq!(state.changes[7].fractional, 1.0);
    state.clear_changes();
    assert_eq!(state.changes[7].fractional, 1.0);
    state.set(Motive::Hunger, 0);
    state.set_change(Motive::Hunger, 4500, 100);
    let mut restored: MotiveState =
        bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
    for _ in 0..9 {
        state.tick_changes(AvatarPlatform::Tso);
        restored.tick_changes(AvatarPlatform::Tso);
    }
    assert_eq!(state, restored);
    assert_eq!(state.get(Motive::Hunger), 5);
}

#[test]
fn avatar_tso_decay_source_narrowing_and_minute_gate() {
    let mut state = MotiveState::default();
    let tuning = TsoMotiveTuning {
        flat_sim: [2, 400, 80, 170, 150, 300, 300, 180000, 16, 250, 55, 0],
        category_weights: [[1000; 7]; 11],
    };
    let mut decay = MotiveDecay::tso(Some(tuning));
    let mut ctx = DecayContext {
        minute: 1,
        room_score: 80,
        ..DecayContext::default()
    };
    decay.tick(&mut state, &ctx).unwrap();
    assert_eq!(state.get(Motive::Room), 80);
    assert_eq!(state.get(Motive::Mood), 97);
    assert_eq!(decay.fractions(), &[400, 400, 170, 420, 187, 250, 27]);
    let snapshot = decay.clone();
    ctx.room_score = 60;
    decay.tick(&mut state, &ctx).unwrap();
    assert_eq!(decay, snapshot);
    assert_eq!(state.get(Motive::Room), 60); // room updates even within same minute
    ctx.cheats = 1;
    ctx.minute = 2;
    decay.tick(&mut state, &ctx).unwrap();
    assert_eq!(decay, snapshot);
}

#[test]
fn avatar_ts1_decay_skips_changed_motive_and_keeps_source_mood_quirk() {
    let mut state = MotiveState::default();
    state.tick_changes(AvatarPlatform::Ts1);
    state.set_change(Motive::Hunger, 1, 100);
    let mut decay = MotiveDecay::ts1();
    let mut ctx = DecayContext {
        minute: 2,
        active_personality: 666,
        ..DecayContext::default()
    };
    decay.tick(&mut state, &ctx).unwrap();
    assert_eq!(state.get(Motive::Mood), 87); // skipped hunger omitted from sum but divisor stays eight
    assert_eq!(decay.fractions()[0], 0);
    assert_eq!(decay.fractions()[1], 500);
    state.set(Motive::SleepState, -1);
    state.set(Motive::Energy, 0);
    ctx.minute = 4;
    decay.tick(&mut state, &ctx).unwrap();
    assert_eq!(state.get(Motive::Energy), 1); // existing +375 fraction, then -1286 => -911, restore one
}

#[test]
fn avatar_motive_rate_scaling_is_integer_and_explicit_about_missing_tuning() {
    let tuning = TsoMotiveTuning {
        flat_sim: [0, 0, 0, 0, 0, 0, 0, 180000, 16, 0, 0, 0],
        category_weights: [[2000; 7]; 11],
    };
    assert_eq!(tuning.scale_rate(10, Motive::Hunger, 4).unwrap(), 7);
    assert_eq!(tuning.scale_rate(10, Motive::Comfort, 4).unwrap(), 15);
    assert_eq!(tuning.scale_rate(-10, Motive::Hunger, 4).unwrap(), -10);
    let mut decay = MotiveDecay::tso(None);
    let ctx = DecayContext {
        minute: 1,
        ..DecayContext::default()
    };
    assert!(decay.tick(&mut MotiveState::default(), &ctx).is_err());
}
