//! Native-testable receipt classification; a retained commit is never fresh evidence.
use wonderland_client_app::authoring::{AuthoringState, PreviewAuthoringProvider};
use wonderland_contracts::authoring::*;
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Ignored,
    Rejected(AuthoringError),
    Committed(AuthoringCommit),
}
pub fn preview_reply(
    provider: &mut PreviewAuthoringProvider,
    request: &AuthoringRequest,
    reject: bool,
) -> AuthoringEvent {
    if reject {
        AuthoringEvent::Rejected {
            operation_id: request.operation_id.clone(),
            base_revision: request.base_revision,
            error: AuthoringError::Rejected(
                "The preview could not save this change. Try again.".into(),
            ),
        }
    } else {
        provider.handle(request)
    }
}
pub fn deliver(
    state: &mut AuthoringState,
    request: &AuthoringRequest,
    event: AuthoringEvent,
) -> Result<Delivery, AuthoringError> {
    let active = state.pending() == Some(request);
    let before = state.projection().revision;
    let rejection = match &event {
        AuthoringEvent::Rejected {
            operation_id,
            base_revision,
            error,
        } if active
            && operation_id == &request.operation_id
            && *base_revision == request.base_revision =>
        {
            Some(error.clone())
        }
        _ => None,
    };
    state.receive(event)?;
    if active
        && state.projection().revision > before
        && let Some(commit) = state.last_commit()
        && commit.operation_id == request.operation_id
        && commit.base_revision == request.base_revision
        && commit.revision == state.projection().revision
    {
        return Ok(Delivery::Committed(commit.clone()));
    }
    if state.pending().is_none()
        && let Some(error) = rejection
    {
        return Ok(Delivery::Rejected(error));
    }
    Ok(Delivery::Ignored)
}

/// Destination copy follows the actual travel request, not another current selection.
pub fn travel_message(
    shell: &wonderland_client_app::ShellState,
    request: &wonderland_contracts::UiRequest,
    pending: bool,
) -> String {
    let name = match &request.kind {
        wonderland_contracts::RequestKind::Travel { place_id, .. } => shell
            .projection
            .places
            .iter()
            .find(|p| &p.id == place_id)
            .map(|p| p.name.as_str())
            .unwrap_or("your destination"),
        _ => "your destination",
    };
    if pending {
        format!("Visiting {name}…")
    } else {
        format!("Welcome to {name}.")
    }
}

/// Recover both character failure fixtures into the same authoring-compatible
/// shell projection used for initial load and accepted authoring receipts.
pub fn recovered_characters(
    current: &wonderland_contracts::UiProjection,
    authoring: &AuthoringProjection,
) -> Result<wonderland_contracts::UiProjection, AuthoringError> {
    wonderland_client_app::authoring::preview_project_authoring_to_ui(current, authoring)
}
