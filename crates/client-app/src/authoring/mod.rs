//! Pure authoring reducer and explicitly bounded fixture/demo provider.
//!
//! Nothing here is production account, economy, simulation, or placement authority.
//! Apply a matching authoring receipt before mirroring its snapshot into the shell.

mod provider;
mod state;
mod transition;

pub use provider::{MAX_PREVIEW_OPERATIONS, PreviewAuthoringProvider};
pub use state::AuthoringState;

use wonderland_contracts::authoring::{AuthoringError, AuthoringProjection};
use wonderland_contracts::{Availability, UiProjection};

/// Repository-owned fixture. It never reads storage, submits work, or starts timers.
pub fn preview_authoring_projection() -> AuthoringProjection {
    let projection: AuthoringProjection =
        serde_json::from_str(include_str!("../../../../fixtures/ui/authoring-v1.json"))
            .expect("checked-in authoring fixture must deserialize");
    projection
        .validate()
        .expect("checked-in authoring fixture must validate");
    projection
}

/// Explicit preview-only bridge. Deliver the result through `UiEvent::ProjectionUpdated`.
/// On commit, call only after `AuthoringState::receive` has accepted the matching receipt.
/// A live adapter must instead provide coordinated authoritative projections.
pub fn preview_project_authoring_to_ui(
    current: &UiProjection,
    authoring: &AuthoringProjection,
) -> Result<UiProjection, AuthoringError> {
    current
        .validate()
        .map_err(|error| AuthoringError::InvalidProjection(error.to_string()))?;
    authoring.validate()?;
    let mut next = current.clone();
    next.revision = current
        .revision
        .checked_add(1)
        .ok_or(AuthoringError::OperationLimit)?;
    next.characters = authoring
        .profiles
        .iter()
        .map(|profile| profile.character.clone())
        .collect();
    let home = next
        .places
        .iter_mut()
        .find(|place| place.id.as_ref() == "home")
        .ok_or_else(|| {
            AuthoringError::InvalidProjection(
                "The compatible preview shell must contain Home".into(),
            )
        })?;
    home.availability = Availability::Available;
    next.validate()
        .map_err(|error| AuthoringError::InvalidProjection(error.to_string()))?;
    Ok(next)
}
