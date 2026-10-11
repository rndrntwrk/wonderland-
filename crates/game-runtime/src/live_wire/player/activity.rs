//! Bounded presentation history from already validated runtime outcomes.
//! Transport receipts are deliberately not an input: acceptance is not completion.
use crate::{EntityRef, TickOutcome};
use std::collections::VecDeque;

pub const MAX_RECENT_ACTIONS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Completed,
    Failed,
    Stopped,
    Cancelled,
    Unavailable,
}
impl ActionResult {
    pub fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Cancelled => "cancelled",
            Self::Unavailable => "became unavailable",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionActivity {
    pub action: u64,
    pub tick: u64,
    pub event_sequence: u64,
    pub result: ActionResult,
}
impl ActionActivity {
    pub fn message(&self) -> String {
        format!("Action #{} {}", self.action, self.result.label())
    }
}

pub(super) struct ActivityLog {
    actor: EntityRef,
    entries: VecDeque<ActionActivity>,
    last_tick: u64,
    last_sequence: u64,
}
impl ActivityLog {
    pub fn new(actor: EntityRef) -> Self {
        Self {
            actor,
            entries: VecDeque::new(),
            last_tick: 0,
            last_sequence: 0,
        }
    }
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &ActionActivity> {
        self.entries.iter()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn observe(&mut self, outcomes: &[TickOutcome]) {
        use crate::sim_core::interactions::{FinishResult, QueueEventKind, RemovalReason};
        use crate::{RuntimeEvent, entity_key};
        for outcome in outcomes {
            if outcome.duplicate || outcome.tick < self.last_tick {
                continue;
            }
            for event in &outcome.events {
                let RuntimeEvent::Interaction(event) = event else {
                    continue;
                };
                if event.actor != entity_key(self.actor) || event.sequence <= self.last_sequence {
                    continue;
                }
                self.last_sequence = event.sequence;
                self.last_tick = outcome.tick;
                let Some(action) = event.action else {
                    continue;
                };
                let result = match event.kind {
                    QueueEventKind::Finished {
                        result: FinishResult::Succeeded,
                    } => ActionResult::Completed,
                    QueueEventKind::Finished {
                        result: FinishResult::Failed,
                    }
                    | QueueEventKind::StartRejected
                    | QueueEventKind::RuntimeFailed { .. } => ActionResult::Failed,
                    QueueEventKind::Finished {
                        result: FinishResult::Aborted,
                    } => ActionResult::Stopped,
                    QueueEventKind::Removed {
                        reason: RemovalReason::Cancelled | RemovalReason::ParentIdleCancelled,
                    } => ActionResult::Cancelled,
                    QueueEventKind::Removed {
                        reason: RemovalReason::CheckRejected | RemovalReason::DeadTarget,
                    } => ActionResult::Unavailable,
                    // Removed(Finished) is cleanup following the actual result,
                    // not evidence of success; CancelRequested is not completion.
                    _ => continue,
                };
                if self
                    .entries
                    .iter()
                    .any(|entry| entry.action == action.0 && entry.result == result)
                {
                    continue;
                }
                self.entries.retain(|entry| entry.action != action.0);
                if self.entries.len() == MAX_RECENT_ACTIONS {
                    self.entries.pop_front();
                }
                self.entries.push_back(ActionActivity {
                    action: action.0,
                    tick: outcome.tick,
                    event_sequence: event.sequence,
                    result,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim_core::interactions::{
        ActionId, FinishResult, QueueEvent, QueueEventKind, RemovalReason,
    };
    use crate::{ObjectId, RuntimeEvent, entity_key};
    fn actor() -> EntityRef {
        EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        }
    }
    fn tick(n: u64, events: Vec<QueueEvent>) -> TickOutcome {
        TickOutcome {
            tick: n,
            duplicate: false,
            state_hash: [0; 32],
            instructions: 0,
            events: events.into_iter().map(RuntimeEvent::Interaction).collect(),
            effects: vec![],
        }
    }
    fn event(sequence: u64, action: u64, kind: QueueEventKind) -> QueueEvent {
        QueueEvent {
            sequence,
            queue_revision: sequence,
            actor: entity_key(actor()),
            action: Some(ActionId(action)),
            kind,
        }
    }
    #[test]
    fn terminal_feedback_comes_from_source_outcomes_not_queue_removal() {
        let mut log = ActivityLog::new(actor());
        log.observe(&[tick(
            9,
            vec![
                event(
                    1,
                    7,
                    QueueEventKind::Finished {
                        result: FinishResult::Succeeded,
                    },
                ),
                event(
                    2,
                    7,
                    QueueEventKind::Removed {
                        reason: RemovalReason::Finished,
                    },
                ),
            ],
        )]);
        assert_eq!(
            log.entries().cloned().collect::<Vec<_>>(),
            vec![ActionActivity {
                action: 7,
                tick: 9,
                event_sequence: 1,
                result: ActionResult::Completed
            }]
        );
    }
    #[test]
    fn cancel_request_never_claims_that_active_cleanup_has_finished() {
        let mut log = ActivityLog::new(actor());
        log.observe(&[tick(5, vec![event(1, 2, QueueEventKind::CancelRequested)])]);
        assert!(log.entries().next().is_none());
        log.observe(&[tick(
            6,
            vec![event(
                2,
                2,
                QueueEventKind::Removed {
                    reason: RemovalReason::Cancelled,
                },
            )],
        )]);
        assert_eq!(
            log.entries().next().unwrap().result,
            ActionResult::Cancelled
        );
    }
    #[test]
    fn completion_failure_and_abort_are_not_conflated() {
        let mut log = ActivityLog::new(actor());
        log.observe(&[tick(
            5,
            vec![
                event(
                    1,
                    1,
                    QueueEventKind::Finished {
                        result: FinishResult::Failed,
                    },
                ),
                event(
                    2,
                    2,
                    QueueEventKind::Finished {
                        result: FinishResult::Aborted,
                    },
                ),
                event(
                    3,
                    3,
                    QueueEventKind::Removed {
                        reason: RemovalReason::DeadTarget,
                    },
                ),
            ],
        )]);
        assert_eq!(
            log.entries().map(|e| e.result).collect::<Vec<_>>(),
            [
                ActionResult::Failed,
                ActionResult::Stopped,
                ActionResult::Unavailable
            ]
        );
    }
    #[test]
    fn actor_generation_duplicates_and_history_are_fenced() {
        let mut log = ActivityLog::new(actor());
        let first = tick(
            10,
            vec![event(
                2,
                1,
                QueueEventKind::Finished {
                    result: FinishResult::Succeeded,
                },
            )],
        );
        let mut other = event(
            900,
            3,
            QueueEventKind::Finished {
                result: FinishResult::Succeeded,
            },
        );
        other.actor.generation += 1;
        log.observe(&[
            first.clone(),
            first,
            tick(
                9,
                vec![event(
                    3,
                    2,
                    QueueEventKind::Finished {
                        result: FinishResult::Succeeded,
                    },
                )],
            ),
            tick(11, vec![other]),
        ]);
        assert_eq!(log.entries().count(), 1);
        // An unrelated actor's large sequence must not poison our high-water.
        log.observe(&[tick(
            12,
            vec![event(
                4,
                2,
                QueueEventKind::Finished {
                    result: FinishResult::Succeeded,
                },
            )],
        )]);
        assert_eq!(log.entries().count(), 2);
    }
    #[test]
    fn recent_action_retention_is_bounded_and_clear_removes_old_player_data() {
        let mut log = ActivityLog::new(actor());
        for n in 1..=100 {
            log.observe(&[tick(
                n,
                vec![event(
                    n,
                    n,
                    QueueEventKind::Finished {
                        result: FinishResult::Succeeded,
                    },
                )],
            )]);
        }
        assert_eq!(log.entries().count(), MAX_RECENT_ACTIONS);
        assert_eq!(log.entries().next().unwrap().action, 69);
        assert_eq!(log.entries().next_back().unwrap().action, 100);
        log.clear();
        assert_eq!(log.entries().count(), 0);
    }
}
