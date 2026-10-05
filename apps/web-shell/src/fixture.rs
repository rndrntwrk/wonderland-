//! Deterministic delayed preview replies, never a live transport.
use wonderland_contracts::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Accept,
    RejectTravel,
    RejectInteraction,
    RejectCancellation,
    EmptyCharacters,
    UnavailableCharacters,
}

pub fn projection(scenario: Scenario) -> UiProjection {
    let mut projection = wonderland_client_app::preview_projection();
    match scenario {
        Scenario::EmptyCharacters => projection.characters.clear(),
        Scenario::UnavailableCharacters => {
            for character in &mut projection.characters {
                character.availability = Availability::Unavailable {
                    reason: "This Sim is taking a break. Try again.".into(),
                };
            }
        }
        _ => {}
    }
    projection
}
pub fn reply(request: &UiRequest, reject: bool) -> UiEvent {
    let operation_id = request.operation_id.clone();
    let projection_revision = request.projection_revision;
    if reject {
        let reason = match request.kind {
            RequestKind::Travel { .. } => "Harbor Café is busy. Try visiting again.",
            RequestKind::Interaction { .. } => "The coffee machine is busy. Try again.",
            RequestKind::Cancellation { .. } => "Couldn't cancel yet. Please try again.",
        };
        UiEvent::Rejected {
            operation_id,
            projection_revision,
            reason: reason.into(),
        }
    } else if matches!(request.kind, RequestKind::Cancellation { .. }) {
        UiEvent::CancellationAcknowledged {
            operation_id,
            projection_revision,
        }
    } else {
        UiEvent::Accepted {
            operation_id,
            projection_revision,
        }
    }
}
pub fn should_reject(scenario: Scenario, kind: &RequestKind) -> bool {
    matches!(
        (scenario, kind),
        (Scenario::RejectTravel, RequestKind::Travel { .. })
            | (Scenario::RejectInteraction, RequestKind::Interaction { .. })
            | (
                Scenario::RejectCancellation,
                RequestKind::Cancellation { .. }
            )
    )
}
