use sim_core::avatars::events::{AnimationCue, TimeProperty};
use sim_core::avatars::timeline::{
    AnimationCommand, AnimationMetadata, AnimationMode, AnimationResult, AnimationTimeline,
};

fn metadata(events: &[(u32, i16)], frames: i32) -> AnimationMetadata {
    AnimationMetadata {
        resource: "fixture.anim".into(),
        num_frames: frames,
        time_properties: events
            .iter()
            .map(|(time, event)| TimeProperty::xevt(*time, *event))
            .collect(),
    }
}

fn command() -> AnimationCommand {
    AnimationCommand::play("fixture.anim")
}

#[test]
fn avatar_animation_preserves_unsorted_resource_order_and_synthesizes_completion() {
    let meta = metadata(&[(60, 7), (0, 101), (20, -3)], 3);
    let mut timeline = AnimationTimeline::default();
    let mut cmd = command();
    cmd.expected_events = 4;
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Wait
    );
    assert!(timeline.tick().unwrap().is_empty()); // The unconsumed C# OrderBy leaves 60 ms at the front.
    assert_eq!(
        timeline
            .tick()
            .unwrap()
            .iter()
            .filter(|cue| matches!(cue, AnimationCue::Xevt { .. }))
            .count(),
        3
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(7)
    );
    timeline.tick().unwrap();
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(101)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(-3)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(2)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(3)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Complete
    );
}

#[test]
fn avatar_animation_reverse_hurry_loop_and_carry_follow_headless_source() {
    let meta = metadata(&[(0, 0), (50, 1), (100, 2)], 3);
    let mut timeline = AnimationTimeline::default();
    let mut cmd = command();
    cmd.backwards = true;
    cmd.hurryable = true;
    cmd.walk_style = 1;
    timeline.apply(&cmd, Some(&meta)).unwrap();
    timeline.tick().unwrap();
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(2)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(1)
    );
    timeline.tick().unwrap();
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Event(0)
    );
    assert_eq!(
        timeline.apply(&cmd, Some(&meta)).unwrap(),
        AnimationResult::Complete
    );

    cmd.mode = AnimationMode::Loop;
    cmd.backwards = false;
    timeline.apply(&cmd, Some(&meta)).unwrap();
    let count: usize = (0..8).map(|_| timeline.tick().unwrap().len()).sum();
    assert_eq!(count, 3); // Source looping does not refill consumed time properties.
    assert!(!timeline.animations[0].end_reached);
    cmd.mode = AnimationMode::Carry;
    timeline.apply(&cmd, Some(&meta)).unwrap();
    timeline.tick().unwrap();
    assert_eq!(timeline.carry.as_ref().unwrap().current_frame, 0.0);
}

#[test]
fn avatar_animation_absent_resource_and_mid_event_snapshot() {
    let cmd = command();
    let mut timeline = AnimationTimeline::default();
    assert_eq!(
        timeline.apply(&cmd, None).unwrap(),
        AnimationResult::CompleteNextTick
    );
    let meta = metadata(&[(0, 1), (30, 2), (75, 3)], 8);
    timeline.apply(&cmd, Some(&meta)).unwrap();
    timeline.tick().unwrap();
    timeline.apply(&cmd, Some(&meta)).unwrap();
    let bytes = bincode::serialize(&timeline).unwrap();
    let mut restored: AnimationTimeline = bincode::deserialize(&bytes).unwrap();
    restored.validate().unwrap();
    for _ in 0..12 {
        assert_eq!(timeline.tick().unwrap(), restored.tick().unwrap());
        assert_eq!(
            timeline.apply(&cmd, Some(&meta)),
            restored.apply(&cmd, Some(&meta))
        );
        assert_eq!(timeline, restored);
    }
}

#[test]
fn avatar_animation_invalid_continuation_is_rejected() {
    let mut timeline = AnimationTimeline::default();
    timeline.apply(&command(), Some(&metadata(&[], 2))).unwrap();
    timeline.animations[0].current_frame = f32::NAN;
    assert!(timeline.validate().is_err());
}

#[test]
fn avatar_animation_aggregate_event_overflow_is_atomic() {
    let mut timeline = AnimationTimeline::default();
    let meta = metadata(&[(0, 1)], 2);
    timeline.apply(&command(), Some(&meta)).unwrap();
    timeline.animations[0]
        .event_queue
        .extend(std::iter::repeat(1).take(65_536 + 255));
    timeline.validate().unwrap();
    let before = timeline.clone();
    assert!(timeline.tick().is_err());
    assert_eq!(timeline, before);
}

#[test]
fn avatar_animation_synthetic_event_overflow_is_atomic() {
    let mut timeline = AnimationTimeline::default();
    let meta = metadata(&[], 2);
    let mut cmd = command();
    cmd.expected_events = 255;
    timeline.apply(&cmd, Some(&meta)).unwrap();
    timeline.animations[0].end_reached = true;
    timeline.animations[0]
        .event_queue
        .extend(std::iter::repeat(100).take(65_536 + 255));
    timeline.validate().unwrap();
    let before = timeline.clone();
    assert!(timeline.apply(&cmd, Some(&meta)).is_err());
    assert_eq!(timeline, before);
}

#[test]
fn avatar_animation_blend_events_target_first_state_and_byte_counter_wraps() {
    use sim_core::avatars::timeline::AnimationState;
    let mut timeline = AnimationTimeline::default();
    let first = metadata(&[], 100);
    timeline.apply(&command(), Some(&first)).unwrap();
    let second = metadata(&vec![(0, -1); 256], 100);
    timeline
        .animations
        .push(AnimationState::new(second, false).unwrap());
    let cues = timeline.tick().unwrap();
    assert_eq!(cues.len(), 256);
    assert_eq!(timeline.animations[0].event_queue.len(), 256);
    assert_eq!(timeline.animations[0].events_run, 0);
    assert!(timeline.animations[1].event_queue.is_empty());
    assert_eq!(timeline.animations[1].events_run, 0);
}
