// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Queue fixtures only: runtime records synthetic check/frame outcomes.
mod fixtures;
use fixtures::*;
use sim_core::interactions as wonderland_interactions_check;
use wonderland_interactions_check::*;

#[test]
fn source_numeric_priorities_modes_and_overloaded_flags_are_stable() {
    assert_eq!(
        [
            QueuePriority::Maximum as i16,
            QueuePriority::Autonomous as i16,
            QueuePriority::UserDriven as i16,
            QueuePriority::ParentIdle as i16,
            QueuePriority::ParentExit as i16,
            QueuePriority::Idle as i16
        ],
        [100, 2, 50, 40, 30, 0]
    );
    assert_eq!(
        [
            QueueMode::Normal as u8,
            QueueMode::ParentIdle as u8,
            QueueMode::ParentExit as u8,
            QueueMode::Idle as u8
        ],
        [0, 1, 2, 3]
    );
    assert_eq!(ActionFlags::LEAPFROG, ActionFlags::TS1_ALLOW_CATS);
    assert_eq!(ActionFlags::MUST_RUN, ActionFlags::TS1_ALLOW_DOGS);
    assert_eq!(ActionFlags::PUSH_TAIL, 1 << 30);
    assert_eq!(ActionFlags::PUSH_HEAD, 1 << 29);
    assert_eq!(ActionFlags::SKIP_PERMISSIONS, 1 << 28);
    assert_eq!(InteractionLimits::default().max_user_queue_entries, 20);
}

#[test]
fn interpreter_push_mapping_sets_only_source_continuation_and_permission_flags() {
    assert_eq!(
        resolve_push_priority(PushPriority::Inherited, None),
        (1, QueueMode::Normal)
    );
    assert_eq!(
        resolve_push_priority(PushPriority::Inherited, Some(-10)),
        (1, QueueMode::Normal)
    );
    assert_eq!(
        resolve_push_priority(PushPriority::Inherited, Some(75)),
        (75, QueueMode::Normal)
    );
    let action = ActionInvocation::pushed(
        actor_key(),
        target_key(),
        definition(1),
        PushPriority::ParentExit,
        None,
        true,
        true,
    );
    assert_eq!((action.priority, action.mode), (30, QueueMode::ParentExit));
    assert!(action.definition.flags.has(ActionFlags::SKIP_PERMISSIONS));
    assert!(action.definition.flags.has(ActionFlags::PUSH_HEAD));
    assert!(action.definition.flags.has(ActionFlags::PUSH_TAIL));
    assert_eq!(action.effective_icon_owner(), target_key());
    assert_eq!(action.args, [0; 4]);
}

#[test]
fn same_priority_queue_is_fifo_and_identical_interactions_are_not_deduplicated() {
    let mut queue = queue(LegacyMode::Tso);
    let runtime = FixtureRuntime::default();
    for index in [3, 1, 1, 2] {
        queue
            .enqueue_internal(invocation(index, 50, QueueMode::Normal, 0), &runtime)
            .unwrap();
    }
    assert_eq!(indices(&queue), vec![3, 1, 1, 2]);
    let ids: Vec<_> = queue.entries().iter().map(|entry| entry.id.0).collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn push_head_overrides_priorities_without_moving_the_active_prefix() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue.attempt_push(&mut runtime).unwrap();
    let active = queue.active().unwrap().id;
    queue
        .enqueue_internal(invocation(2, 100, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue
        .enqueue_internal(
            invocation(3, 0, QueueMode::Normal, ActionFlags::PUSH_HEAD),
            &runtime,
        )
        .unwrap();
    assert_eq!(indices(&queue), vec![1, 3, 2]);
    assert_eq!(queue.active().unwrap().id, active);
    assert_eq!(queue.active_len(), 1);
    assert!(queue.interaction_cancelled());
    assert_eq!(
        queue.attempt_push(&mut runtime).unwrap().result,
        PushOutcome::PriorityBlocked
    );
}

#[test]
fn tail_and_tso_leapfrog_step_before_lower_priority_parent_exit_but_ts1_bit_does_not() {
    for (mode, flag, expected) in [
        (LegacyMode::Tso, ActionFlags::PUSH_TAIL, vec![3, 1, 2]),
        (LegacyMode::Tso, ActionFlags::LEAPFROG, vec![3, 1, 2]),
        (LegacyMode::Ts1, ActionFlags::LEAPFROG, vec![1, 2, 3]),
    ] {
        let mut queue = queue(mode);
        let runtime = FixtureRuntime::default();
        queue
            .enqueue_internal(invocation(1, 30, QueueMode::ParentExit, 0), &runtime)
            .unwrap();
        queue
            .enqueue_internal(invocation(2, 50, QueueMode::Normal, 0), &runtime)
            .unwrap();
        queue
            .enqueue_internal(invocation(3, 50, QueueMode::Normal, flag), &runtime)
            .unwrap();
        assert_eq!(indices(&queue), expected);
    }
    let mut queue = queue(LegacyMode::Tso);
    let runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(1, 30, QueueMode::ParentExit, 0), &runtime)
        .unwrap();
    queue
        .enqueue_internal(
            invocation(2, 100, QueueMode::ParentExit, ActionFlags::PUSH_TAIL),
            &runtime,
        )
        .unwrap();
    assert_eq!(
        indices(&queue),
        vec![1, 2],
        "parent-exit mode disables tail/leapfrog placement"
    );
}

#[test]
fn parent_idle_cancellation_removes_following_idles_but_runs_cancelled_parent_exit_cleanup() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(3, 30, QueueMode::ParentExit, 0), &runtime)
        .unwrap();
    queue
        .enqueue_internal(invocation(4, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    let first_idle = queue
        .enqueue_internal(invocation(1, 40, QueueMode::ParentIdle, 0), &runtime)
        .unwrap()
        .result;
    queue
        .enqueue_internal(invocation(2, 40, QueueMode::ParentIdle, 0), &runtime)
        .unwrap();
    assert_eq!(indices(&queue), vec![1, 2, 3, 4]);
    assert_eq!(
        [
            queue.entry_visible(0),
            queue.entry_visible(1),
            queue.entry_visible(2),
            queue.entry_visible(3)
        ],
        [Some(true), Some(false), Some(false), Some(true)]
    );
    let cancel = queue.cancel_internal(first_idle).unwrap();
    assert_eq!(cancel.result, CancelOutcome::Retained);
    assert_eq!(indices(&queue), vec![1, 3, 4]);
    let exit = &queue.entries()[1];
    assert_eq!(exit.invocation.priority, 0);
    assert!(exit.notify_idle);
    let result = queue.attempt_push(&mut runtime).unwrap();
    assert!(matches!(result.result, PushOutcome::Started(_)));
    assert_eq!(indices(&queue), vec![3, 4]);
    assert_eq!(
        queue.active().unwrap().invocation.mode,
        QueueMode::ParentExit
    );
    assert_eq!(queue.current_priority(), 0);
    assert_eq!(
        runtime.checked,
        vec![3],
        "cancelled parent idle bypasses its check; exit still checks and starts"
    );
    assert!(!queue.interaction_cancelled());
}

#[test]
fn cancellation_retains_running_and_must_run_entries_but_removes_waiting_normal_actions() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    let active = queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap()
        .result;
    queue.attempt_push(&mut runtime).unwrap();
    let pending = queue
        .enqueue_internal(invocation(2, 50, QueueMode::Normal, 0), &runtime)
        .unwrap()
        .result;
    let mandatory = queue
        .enqueue_internal(
            invocation(3, 50, QueueMode::Normal, ActionFlags::MUST_RUN),
            &runtime,
        )
        .unwrap()
        .result;
    assert_eq!(
        queue.cancel_internal(pending).unwrap().result,
        CancelOutcome::Removed
    );
    assert_eq!(
        queue.cancel_internal(mandatory).unwrap().result,
        CancelOutcome::Retained
    );
    assert_eq!(
        queue.cancel_internal(active).unwrap().result,
        CancelOutcome::Retained
    );
    assert_eq!(indices(&queue), vec![1, 3]);
    assert_eq!(queue.active_len(), 1);
    assert_eq!(queue.current_priority(), 0);
    assert!(queue.interaction_cancelled());
    assert!(queue.entries().iter().all(|entry| entry.notify_idle));
}

#[test]
fn direct_control_can_skip_hidden_idle_and_emits_disconnect_before_cancel_at_index_zero() {
    let mut queue = queue(LegacyMode::Tso);
    let runtime = FixtureRuntime::default();
    let id = queue
        .enqueue_internal(
            invocation(1, 1, QueueMode::Idle, ActionFlags::DIRECT_CONTROL),
            &runtime,
        )
        .unwrap()
        .result;
    assert_eq!(queue.entry_visible(0), Some(false));
    let cancelled = queue.cancel_internal(id).unwrap();
    assert_eq!(cancelled.result, CancelOutcome::Removed);
    assert_eq!(
        cancelled.events[0].kind,
        QueueEventKind::EodDisconnectRequested
    );
    assert_eq!(cancelled.events[1].kind, QueueEventKind::CancelRequested);
    assert_eq!(
        cancelled.events.last().unwrap().kind,
        QueueEventKind::Removed {
            reason: RemovalReason::Cancelled
        }
    );
}

#[test]
fn ts1_queue_skips_original_tail_once_and_preserves_must_run_without_spinning() {
    let mut queue = queue(LegacyMode::Ts1);
    let runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(1, 100, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue
        .enqueue_internal(invocation(2, 20, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue
        .enqueue_internal(
            invocation(3, 10, QueueMode::Normal, ActionFlags::MUST_RUN),
            &runtime,
        )
        .unwrap();
    let change = queue
        .enqueue_internal(invocation(4, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    assert_eq!(indices(&queue), vec![1, 4, 3]);
    assert!(queue.entries()[2].notify_idle);
    assert!(change.events.iter().any(|event| event.kind
        == QueueEventKind::QueueSkippedEntryPointRequested {
            target: target_key()
        }));
    assert_eq!(
        change
            .events
            .iter()
            .filter(|event| event.kind == QueueEventKind::CancelRequested)
            .count(),
        2
    );
}

#[test]
fn failed_check_prunes_only_rejected_pending_action_and_can_start_following_action() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    runtime.reject_check.insert(1);
    queue
        .enqueue_internal(
            invocation(1, 50, QueueMode::Normal, ActionFlags::MUST_RUN),
            &runtime,
        )
        .unwrap();
    let second = queue
        .enqueue_internal(invocation(2, 50, QueueMode::Normal, 0), &runtime)
        .unwrap()
        .result;
    let result = queue.attempt_push(&mut runtime).unwrap();
    assert_eq!(result.result, PushOutcome::Started(second));
    assert_eq!(indices(&queue), vec![2]);
    assert_eq!(runtime.checked, vec![1, 2]);
    assert_eq!(runtime.frames, vec![2]);
    assert_eq!(queue.active_len(), 1);
}

#[test]
fn rejected_frame_start_keeps_action_and_source_priority_without_fictitious_frame() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    runtime.refuse_start.insert(1);
    let id = queue
        .enqueue_internal(
            invocation(1, 50, QueueMode::Normal, ActionFlags::MUST_RUN),
            &runtime,
        )
        .unwrap()
        .result;
    let result = queue.attempt_push(&mut runtime).unwrap();
    assert_eq!(result.result, PushOutcome::StartRejected(id));
    assert_eq!(queue.active_len(), 0);
    assert_eq!(indices(&queue), vec![1]);
    assert_eq!(
        queue.current_priority(),
        50,
        "ExecuteAction writes Priority before Push returns false"
    );
    assert!(runtime.frames.is_empty());
    runtime.refuse_start.clear();
    queue.set_current_priority(0, &runtime).unwrap();
    assert_eq!(
        queue.attempt_push(&mut runtime).unwrap().result,
        PushOutcome::Started(id)
    );
    assert_eq!(queue.active_len(), runtime.frames.len());
}

#[test]
fn runtime_error_preserves_candidate_and_reports_ordered_failure_after_prior_rejection() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    runtime.reject_check.insert(1);
    runtime.check_errors.insert(2);
    queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    let id = queue
        .enqueue_internal(
            invocation(2, 50, QueueMode::Normal, ActionFlags::MUST_RUN),
            &runtime,
        )
        .unwrap()
        .result;
    let result = queue.attempt_push(&mut runtime).unwrap();
    assert!(
        matches!(result.result, PushOutcome::RuntimeFailed { action, stage: RuntimeStage::Check, .. } if action == id)
    );
    assert_eq!(indices(&queue), vec![2]);
    assert_eq!(queue.active_len(), 0);
    assert!(matches!(
        result.events.first().unwrap().kind,
        QueueEventKind::Removed {
            reason: RemovalReason::CheckRejected
        }
    ));
    assert!(matches!(
        result.events.last().unwrap().kind,
        QueueEventKind::RuntimeFailed {
            stage: RuntimeStage::Check,
            ..
        }
    ));
}

#[test]
fn route_failure_or_abort_finishes_only_active_leaf_preserving_parent_exit_cleanup() {
    for completion in [FinishResult::Failed, FinishResult::Aborted] {
        let mut queue = queue(LegacyMode::Tso);
        let mut runtime = FixtureRuntime::default();
        let mut first = invocation(1, 50, QueueMode::Normal, 0);
        first.callback = Some(123);
        queue.enqueue_internal(first, &runtime).unwrap();
        queue.attempt_push(&mut runtime).unwrap();
        queue
            .enqueue_internal(
                invocation(2, 30, QueueMode::ParentExit, ActionFlags::MUST_RUN),
                &runtime,
            )
            .unwrap();
        assert_eq!(runtime.frames.pop(), Some(1));
        let result = queue.finish_active(completion, &runtime).unwrap();
        assert_eq!(indices(&queue), vec![2]);
        assert_eq!(queue.active_len(), 0);
        assert_eq!(queue.current_priority(), 0);
        assert_eq!(
            result
                .events
                .iter()
                .filter(|event| event.kind == QueueEventKind::CallbackRequested { callback: 123 })
                .count(),
            1
        );
        assert_eq!(
            result
                .events
                .iter()
                .filter(|event| event.kind == QueueEventKind::ResetAvatarInteractionStateRequested)
                .count(),
            1
        );
        assert!(matches!(
            queue.attempt_push(&mut runtime).unwrap().result,
            PushOutcome::Started(_)
        ));
        assert_eq!(
            queue.active().unwrap().invocation.mode,
            QueueMode::ParentExit
        );
        assert_eq!(runtime.frames, vec![2]);
    }
}

#[test]
fn run_immediately_is_hidden_until_injection_and_does_not_overwrite_parent_priority() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue.attempt_push(&mut runtime).unwrap();
    let immediate = queue
        .enqueue_internal(
            invocation(2, 1, QueueMode::Normal, ActionFlags::RUN_IMMEDIATELY),
            &runtime,
        )
        .unwrap()
        .result;
    queue
        .enqueue_internal(
            invocation(3, 2, QueueMode::Normal, ActionFlags::RUN_IMMEDIATELY),
            &runtime,
        )
        .unwrap();
    assert_eq!(queue.entries()[1].invocation.mode, QueueMode::Idle);
    assert_eq!(queue.entry_visible(1), Some(false));
    assert_eq!(
        queue.try_run_immediately(&mut runtime).unwrap().result,
        PushOutcome::Started(immediate)
    );
    assert_eq!(queue.active_len(), 2);
    assert_eq!(queue.current_priority(), 50);
    assert_eq!(
        queue.try_run_immediately(&mut runtime).unwrap().result,
        PushOutcome::PriorityBlocked
    );
    assert_eq!(runtime.frames.pop(), Some(2));
    let finish = queue
        .finish_active(FinishResult::Succeeded, &runtime)
        .unwrap();
    assert!(!finish
        .events
        .iter()
        .any(|event| event.kind == QueueEventKind::ResetAvatarInteractionStateRequested));
    assert_eq!(queue.active_len(), 1);
    assert_eq!(
        queue.active().unwrap().invocation.definition.key.tta_index,
        1
    );
    assert!(matches!(
        queue.try_run_immediately(&mut runtime).unwrap().result,
        PushOutcome::Started(_)
    ));
    assert_eq!(queue.active_len(), runtime.frames.len());
}

#[test]
fn immediate_start_failure_never_extends_active_prefix_and_keeps_cleanup() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    runtime.refuse_start.insert(2);
    queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue.attempt_push(&mut runtime).unwrap();
    let id = queue
        .enqueue_internal(
            invocation(2, 50, QueueMode::Normal, ActionFlags::RUN_IMMEDIATELY),
            &runtime,
        )
        .unwrap()
        .result;
    queue
        .enqueue_internal(invocation(3, 30, QueueMode::ParentExit, 0), &runtime)
        .unwrap();
    let before = indices(&queue);
    assert_eq!(
        queue.try_run_immediately(&mut runtime).unwrap().result,
        PushOutcome::StartRejected(id)
    );
    assert_eq!(indices(&queue), before);
    assert_eq!(queue.active_len(), 1);
    assert_eq!(runtime.frames, vec![1]);
}

#[test]
fn cancelling_active_parent_idle_keeps_active_descendant_frames_and_parent_exit() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    let parent = queue
        .enqueue_internal(invocation(1, 40, QueueMode::ParentIdle, 0), &runtime)
        .unwrap()
        .result;
    queue.attempt_push(&mut runtime).unwrap();
    queue
        .enqueue_internal(
            invocation(2, 100, QueueMode::ParentIdle, ActionFlags::PUSH_HEAD),
            &runtime,
        )
        .unwrap();
    // Enqueue marks parent cancelled; a real Allow Push resets/changes state as
    // appropriate. Check-thread queue bypasses the parent-idle check; here a
    // normal child offers a clean frame-invariant scenario through immediate.
    let child = queue
        .enqueue_internal(
            invocation(3, 100, QueueMode::Normal, ActionFlags::RUN_IMMEDIATELY),
            &runtime,
        )
        .unwrap()
        .result;
    queue.try_run_immediately(&mut runtime).unwrap();
    queue
        .enqueue_internal(invocation(4, 30, QueueMode::ParentExit, 0), &runtime)
        .unwrap();
    assert_eq!(queue.active_len(), 2);
    queue.cancel_internal(parent).unwrap();
    assert_eq!(queue.active_len(), 2);
    assert_eq!(queue.active().unwrap().id, child);
    assert_eq!(runtime.frames, vec![1, 3]);
    assert!(!indices(&queue).contains(&2));
    assert!(queue
        .entries()
        .iter()
        .any(|entry| entry.invocation.mode == QueueMode::ParentExit
            && entry.invocation.priority == 0));
}

#[test]
fn impossible_parent_idle_cancel_faults_before_deleting_an_active_parent_idle_descendant() {
    let mut queue =
        ActionQueue::new_check_thread(actor_key(), LegacyMode::Tso, InteractionLimits::default())
            .unwrap();
    let mut runtime = FixtureRuntime::default();
    let parent = queue
        .enqueue_internal(invocation(1, 40, QueueMode::ParentIdle, 0), &runtime)
        .unwrap()
        .result;
    queue.attempt_push(&mut runtime).unwrap();
    queue
        .enqueue_internal(
            invocation(2, 100, QueueMode::ParentIdle, ActionFlags::PUSH_HEAD),
            &runtime,
        )
        .unwrap();
    queue.attempt_push(&mut runtime).unwrap();
    assert_eq!(queue.active_len(), 2);
    let before = queue.entries().to_vec();
    let revision = queue.revision();
    assert_eq!(
        queue.cancel_internal(parent),
        Err(Error::InvalidSnapshot(
            "parent-idle cancellation would remove an active descendant"
        ))
    );
    assert_eq!(queue.entries(), before);
    assert_eq!(queue.revision(), revision);
    assert_eq!(queue.active_len(), runtime.frames.len());
}

#[test]
fn directly_cancelled_parent_exit_is_retained_and_still_executes() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    let id = queue
        .enqueue_internal(invocation(1, 30, QueueMode::ParentExit, 0), &runtime)
        .unwrap()
        .result;
    assert_eq!(
        queue.cancel_internal(id).unwrap().result,
        CancelOutcome::Retained
    );
    assert_eq!(queue.entries()[0].invocation.priority, 30);
    assert!(queue.entries()[0].notify_idle);
    assert_eq!(
        queue.attempt_push(&mut runtime).unwrap().result,
        PushOutcome::Started(id)
    );
    assert_eq!(queue.current_priority(), 30);
    assert_eq!(runtime.frames, vec![1]);
}

#[test]
fn priority_refresh_allows_autonomous_interruption_and_prunes_only_pending_dead_targets() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    queue
        .enqueue_internal(invocation(1, 2, QueueMode::Normal, 0), &runtime)
        .unwrap();
    queue.attempt_push(&mut runtime).unwrap();
    queue
        .enqueue_internal(invocation(2, 2, QueueMode::Normal, 0), &runtime)
        .unwrap();
    assert!(queue.active().unwrap().notify_idle);
    assert!(queue.interaction_cancelled());
    runtime.dead.insert(target_key());
    let update = queue.refresh_priorities(&runtime).unwrap();
    assert_eq!(indices(&queue), vec![1]);
    assert_eq!(queue.active_len(), 1);
    assert!(update.events.iter().any(|event| event.kind
        == QueueEventKind::Removed {
            reason: RemovalReason::DeadTarget
        }));
}

#[test]
fn check_thread_bypasses_check_action_and_does_not_hide_run_immediately_entries() {
    let mut queue =
        ActionQueue::new_check_thread(actor_key(), LegacyMode::Tso, InteractionLimits::default())
            .unwrap();
    let mut runtime = FixtureRuntime::default();
    runtime.reject_check.insert(1);
    queue
        .enqueue_internal(
            invocation(1, 50, QueueMode::Normal, ActionFlags::RUN_IMMEDIATELY),
            &runtime,
        )
        .unwrap();
    assert_eq!(queue.entries()[0].invocation.mode, QueueMode::Normal);
    assert!(matches!(
        queue.attempt_push(&mut runtime).unwrap().result,
        PushOutcome::Started(_)
    ));
    assert!(runtime.checked.is_empty());
}

#[test]
fn queue_limit_rejections_unknown_cancellation_and_empty_finish_are_atomic() {
    let limits = InteractionLimits {
        max_queue_entries: 2,
        max_user_queue_entries: 2,
        ..Default::default()
    };
    let mut queue = ActionQueue::new(actor_key(), LegacyMode::Tso, limits).unwrap();
    let runtime = FixtureRuntime::default();
    for index in [1, 2] {
        queue
            .enqueue_internal(
                invocation(index, 30, QueueMode::ParentExit, ActionFlags::MUST_RUN),
                &runtime,
            )
            .unwrap();
    }
    let before = queue.entries().to_vec();
    let revision = queue.revision();
    assert_eq!(
        queue.enqueue_internal(
            invocation(3, 100, QueueMode::Normal, ActionFlags::PUSH_HEAD),
            &runtime
        ),
        Err(Error::QueueFull)
    );
    assert_eq!(
        queue.cancel_internal(ActionId(999)),
        Err(Error::MissingAction(999))
    );
    assert_eq!(
        queue.finish_active(FinishResult::Aborted, &runtime),
        Err(Error::NoActiveAction)
    );
    assert_eq!(queue.entries(), before);
    assert_eq!(queue.revision(), revision);
}

#[test]
fn transition_events_are_totally_ordered_revision_stamped_and_not_retained_in_queue() {
    let mut queue = queue(LegacyMode::Tso);
    let mut runtime = FixtureRuntime::default();
    let mut events = Vec::new();
    let first = queue
        .enqueue_internal(invocation(1, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    events.extend(first.events);
    events.extend(queue.attempt_push(&mut runtime).unwrap().events);
    events.extend(queue.cancel_internal(first.result).unwrap().events);
    runtime.frames.pop();
    events.extend(
        queue
            .finish_active(FinishResult::Aborted, &runtime)
            .unwrap()
            .events,
    );
    let empty_poll_revision = queue.revision();
    let poll = queue.attempt_push(&mut runtime).unwrap();
    assert!(poll.events.is_empty());
    assert_eq!(queue.revision(), empty_poll_revision);
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.sequence, index as u64 + 1);
        assert_eq!(event.actor, actor_key());
    }
    assert!(events
        .windows(2)
        .all(|pair| pair[0].queue_revision <= pair[1].queue_revision));
    assert!(queue.entries().is_empty());
}
