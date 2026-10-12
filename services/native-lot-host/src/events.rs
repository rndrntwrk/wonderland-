//! Supervisor-owned consequences are committed with the accepted tick stream.
use serde::{Deserialize, Serialize};
use wonderland_game_runtime::{
    RuntimeEvent,
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
