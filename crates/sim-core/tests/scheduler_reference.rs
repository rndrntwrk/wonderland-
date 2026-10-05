use sim_core::ids::{EntityRef, ObjectId};
use sim_core::scheduler::{ScheduleError, Scheduler};
fn e(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}

#[test]
fn sorted_execution_deduplicates_and_keeps_future_sleep() {
    let mut s = Scheduler::new(0);
    for id in [7, 1, 5, 1] {
        s.schedule(e(id), 1).unwrap();
    }
    s.schedule(e(2), 5).unwrap();
    s.begin_tick(1).unwrap();
    assert_eq!(
        [
            s.next_entity(),
            s.next_entity(),
            s.next_entity(),
            s.next_entity()
        ],
        [Some(e(1)), Some(e(5)), Some(e(7)), None]
    );
    assert_eq!(s.scheduled_tick(e(2)), Some(5));
    assert!(s.finish_tick().unwrap().is_empty());
    s.validate(1).unwrap();
}

#[test]
fn interrupt_higher_id_runs_same_tick_lower_id_waits() {
    let mut s = Scheduler::new(0);
    s.schedule(e(5), 1).unwrap();
    s.schedule(e(2), 10).unwrap();
    s.schedule(e(8), 10).unwrap();
    s.begin_tick(1).unwrap();
    assert_eq!(s.next_entity(), Some(e(5)));
    assert_eq!(s.interrupt(e(8)).unwrap(), Some(1));
    assert_eq!(s.interrupt(e(2)).unwrap(), Some(2));
    assert_eq!(s.next_entity(), Some(e(8)));
    assert_eq!(s.next_entity(), None);
    s.finish_tick().unwrap();
    s.validate(1).unwrap();
    s.begin_tick(2).unwrap();
    assert_eq!(s.next_entity(), Some(e(2)));
}

#[test]
fn unscheduled_interrupt_does_not_create_new_thread() {
    let mut s = Scheduler::new(0);
    assert_eq!(s.interrupt(e(7)).unwrap(), None);
    assert_eq!(s.scheduled_tick(e(7)), None);
}

#[test]
fn repeated_interrupt_in_current_tick_does_not_run_twice() {
    let mut s = Scheduler::new(0);
    s.schedule(e(3), 1).unwrap();
    s.begin_tick(1).unwrap();
    assert_eq!(s.next_entity(), Some(e(3)));
    assert_eq!(s.interrupt(e(3)).unwrap(), None);
    assert_eq!(s.next_entity(), None);
}

#[test]
fn every_frame_clamps_long_sleep_but_object_sleep_remains_long() {
    let mut s = Scheduler::new(40);
    s.schedule_in(e(1), 90, true).unwrap();
    s.schedule_in(e(2), 90, false).unwrap();
    assert_eq!(s.scheduled_tick(e(1)), Some(41));
    assert_eq!(s.scheduled_tick(e(2)), Some(130));
}

#[test]
fn deletion_is_deferred_sorted_and_cancels_future_wakes() {
    let mut s = Scheduler::new(0);
    for id in [3, 1, 2] {
        s.schedule(e(id), 1).unwrap();
    }
    s.begin_tick(1).unwrap();
    s.delete_at_end(e(3));
    s.delete_at_end(e(1));
    assert_eq!(s.next_entity(), Some(e(1))); // source keeps objects through the tick
    s.schedule(e(3), 9).unwrap();
    assert_eq!(s.next_entity(), Some(e(2)));
    assert_eq!(s.next_entity(), None);
    assert_eq!(s.finish_tick().unwrap(), vec![e(1), e(3)]);
    assert_eq!(s.scheduled_tick(e(3)), None);
    s.validate(1).unwrap();
}

#[test]
fn ending_a_tick_with_unexecuted_objects_fails_atomically() {
    let mut s = Scheduler::new(0);
    s.schedule(e(1), 1).unwrap();
    s.schedule(e(2), 1).unwrap();
    s.begin_tick(1).unwrap();
    assert_eq!(s.next_entity(), Some(e(1)));
    let before = s.clone();
    assert!(s.finish_tick().is_err());
    assert_eq!(s, before);
    assert_eq!(s.next_entity(), Some(e(2)));
    s.finish_tick().unwrap();
    s.validate(1).unwrap();
}

#[test]
fn schedule_replaces_old_calendar_entry() {
    let mut s = Scheduler::new(0);
    s.schedule(e(1), 2).unwrap();
    s.schedule(e(1), 5).unwrap();
    for tick in 1..=4 {
        s.begin_tick(tick).unwrap();
        assert_eq!(s.next_entity(), None);
        s.finish_tick().unwrap();
    }
    s.begin_tick(5).unwrap();
    assert_eq!(s.next_entity(), Some(e(1)));
}

#[test]
fn invalid_time_and_identity_are_rejected_without_mutation() {
    let mut s = Scheduler::new(4);
    let before = s.clone();
    assert_eq!(s.schedule(e(1), 4), Err(ScheduleError::PastTick));
    assert_eq!(s.schedule(e(0), 5), Err(ScheduleError::InvalidEntity));
    assert_eq!(s.begin_tick(6), Err(ScheduleError::TickGap));
    assert_eq!(s, before);
}

#[test]
fn current_tick_insertion_cannot_rerun_an_already_passed_id() {
    let mut s = Scheduler::new(0);
    s.schedule(e(4), 1).unwrap();
    s.begin_tick(1).unwrap();
    s.next_entity();
    assert_eq!(s.schedule(e(2), 1), Err(ScheduleError::PastTick));
    s.schedule(e(7), 1).unwrap();
    assert_eq!(s.next_entity(), Some(e(7)));
}

#[test]
fn generations_have_distinct_cancellation_identity() {
    let mut s = Scheduler::new(0);
    let old = e(1);
    let new = EntityRef {
        generation: 2,
        ..old
    };
    s.schedule(new, 2).unwrap();
    s.cancel(old);
    assert_eq!(s.scheduled_tick(new), Some(2));
}

#[test]
fn clock_overflow_is_an_explicit_error() {
    let mut s = Scheduler::new(u64::MAX);
    assert_eq!(
        s.schedule_in(e(1), 1, false),
        Err(ScheduleError::TickOverflow)
    );
    assert_eq!(s.begin_tick(0), Err(ScheduleError::TickOverflow));
}

#[test]
fn snapshot_calendar_round_trip_retains_order_and_sleep() {
    let mut a = Scheduler::new(18);
    a.schedule(e(9), 19).unwrap();
    a.schedule(e(4), 25).unwrap();
    let bytes = bincode::serialize(&a).unwrap();
    let mut b: Scheduler = bincode::deserialize(&bytes).unwrap();
    b.validate(18).unwrap();
    let mut executed = Vec::new();
    for tick in 19..=25 {
        a.begin_tick(tick).unwrap();
        b.begin_tick(tick).unwrap();
        while let Some(next) = a.next_entity() {
            executed.push((tick, next));
            assert_eq!(b.next_entity(), Some(next));
        }
        assert_eq!(b.next_entity(), None);
        assert_eq!(a.finish_tick(), b.finish_tick());
        assert_eq!(a, b);
    }
    assert_eq!(executed, vec![(19, e(9)), (25, e(4))]);
}
