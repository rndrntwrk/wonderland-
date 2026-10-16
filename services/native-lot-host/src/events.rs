//! Supervisor-owned consequences are committed with the accepted tick stream.
use serde::{Deserialize, Serialize};
use wonderland_game_runtime::{
    EntityRef, RuntimeEvent,
    sim_core::runtime::leave_adapter::DepartureRemovalReason,
    sim_core::world::{
        build::{BuildCommitResult, DurableBuildRequest},
        routing::RouteCallback,
    },
};
#[derive(Debug, Serialize, Deserialize)]
pub enum SupervisorEvent {
    BuildRequested(DurableBuildRequest),
    BuildCompleted(BuildCommitResult),
    BuildRuntimeFault {
        operation: u64,
        error: String,
    },
    RouteScript {
        continuation_id: u64,
        callback: RouteCallback,
    },
    // Append-only: preserve existing bincode event discriminants.
    OwnedDepartureCaptured {
        actor: EntityRef,
        control_generation: u64,
        tick: u64,
    },
    OwnedDepartureDetached {
        actor: EntityRef,
        control_generation: u64,
        tick: u64,
        reason: DepartureRemovalReason,
    },
    /// Retained idempotent, generation-fenced consequence. This is a request,
    /// never an acknowledgement that an actual EOD association was torn down.
    OwnedEodDisconnectRequested {
        actor: EntityRef,
        control_generation: u64,
    },
}
#[derive(Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub tick: u64,
    pub ordinal: u32,
    pub event: SupervisorEvent,
}
pub fn retain(tick: u64, events: Vec<RuntimeEvent>, output: &mut Vec<EventEnvelope>) {
    for (ordinal, event) in events.into_iter().enumerate() {
        let event = match event {
            RuntimeEvent::BuildRequested(v) => SupervisorEvent::BuildRequested(v),
            RuntimeEvent::BuildCompleted(v) => SupervisorEvent::BuildCompleted(v),
            RuntimeEvent::BuildRuntimeFault { operation, error } => {
                SupervisorEvent::BuildRuntimeFault { operation, error }
            }
            RuntimeEvent::RouteScript {
                continuation_id,
                callback,
            } => SupervisorEvent::RouteScript {
                continuation_id,
                callback,
            },
            RuntimeEvent::OwnedDepartureCaptured {
                actor,
                control_generation,
                tick,
            } => SupervisorEvent::OwnedDepartureCaptured {
                actor,
                control_generation,
                tick,
            },
            RuntimeEvent::OwnedDepartureDetached {
                actor,
                control_generation,
                tick,
                reason,
            } => SupervisorEvent::OwnedDepartureDetached {
                actor,
                control_generation,
                tick,
                reason,
            },
            RuntimeEvent::OwnedEodDisconnectRequested {
                actor,
                control_generation,
            } => SupervisorEvent::OwnedEodDisconnectRequested {
                actor,
                control_generation,
            },
            // External requests already have stable IDs in EffectDispatch. Other
            // events are projections/presentation reconstructed by accepted replay.
            _ => continue,
        };
        output.push(EventEnvelope {
            tick,
            ordinal: ordinal as u32,
            event,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;
    use wonderland_game_runtime::ObjectId;
    #[test]
    fn owned_events_keep_real_ordinals_and_eod_generation_without_an_acknowledgement() {
        let actor = EntityRef {
            object_id: ObjectId(7),
            generation: 9,
        };
        let mut retained = Vec::new();
        retain(
            11,
            vec![
                RuntimeEvent::Spawned(actor),
                RuntimeEvent::OwnedEodDisconnectRequested {
                    actor,
                    control_generation: u64::MAX,
                },
                RuntimeEvent::OwnedDepartureCaptured {
                    actor,
                    control_generation: u64::MAX,
                    tick: 11,
                },
                RuntimeEvent::Deleted(actor),
                RuntimeEvent::OwnedDepartureDetached {
                    actor,
                    control_generation: u64::MAX,
                    tick: 11,
                    reason: DepartureRemovalReason::ForcedCleanup,
                },
            ],
            &mut retained,
        );
        assert_eq!(
            retained
                .iter()
                .map(|event| event.ordinal)
                .collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        let codec = bincode::DefaultOptions::new().with_fixint_encoding();
        let decoded: Vec<EventEnvelope> = codec
            .deserialize(&codec.serialize(&retained).unwrap())
            .unwrap();
        assert!(
            matches!(decoded[0].event, SupervisorEvent::OwnedEodDisconnectRequested {
            actor: observed, control_generation: u64::MAX,
        } if observed == actor)
        );
        // Existing supervisor variants occupy tags 0..=3; new tags append.
        assert_eq!(
            &codec.serialize(&decoded[0].event).unwrap()[..4],
            &6_u32.to_le_bytes()
        );
    }
}
