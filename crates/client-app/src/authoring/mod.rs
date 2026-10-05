//! Pure authoring reducer and explicitly bounded fixture/demo provider.
//!
//! Nothing here is production account, economy, simulation, or placement authority.
//! Apply a matching authoring receipt before mirroring its snapshot into the shell.

mod provider;
mod state;
mod transition;

pub use provider::{MAX_PREVIEW_OPERATIONS, PreviewAuthoringProvider};
pub use state::AuthoringState;

use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, UiProjection};

/// Repository-owned fixture. It never reads storage, submits work, or starts timers.
pub fn preview_authoring_projection() -> AuthoringProjection {
    let projection: AuthoringProjection =
        serde_json::from_str(include_str!("../../../../fixtures/ui/authoring-v2.json"))
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

/// Explicit adapter for the original server policy. Supplying this capability
/// does not authenticate an account; unknown connection state remains separate.
pub fn legacy_account_capabilities() -> AccountCapabilities {
    AccountCapabilities {
        source: "legacy-account-service".into(),
        revision: 1,
        account_id: None,
        shards: vec![],
        default_shard: None,
        creation: Availability::Available,
        profile_capacity: CapacityPolicy::Limited { maximum: 3 },
        fields: ProfileFieldPolicy {
            minimum_name_characters: 3,
            maximum_name_characters: 24,
            name_alphabet: NameAlphabet::AsciiLettersAndSpaces,
            maximum_description_characters: 499,
        },
    }
}

/// Opt into local simulation after reading an old preview save. The generic
/// migration itself never supplies account authority or an unlimited grant.
pub fn resume_preview_projection(mut projection: AuthoringProjection) -> AuthoringProjection {
    if projection.account.source.as_ref() == "legacy-preview-migration" {
        let fixture = preview_authoring_projection();
        projection.account = fixture.account;
        projection.appearance_content = fixture.appearance_content;
        for profile in &mut projection.profiles {
            profile.home.instance_capacity = CapacityPolicy::Unlimited;
            profile.home.build = fixture.profiles[0].home.build.clone();
        }
    }
    // Published v2 fixture saves predate actor grants and explicit object
    // ownership. Only the opted-in, known preview adapter may recover those
    // historical owner-only facts; generic/live projections remain unknown.
    if projection.account.source.as_ref() == "fixture-simulation" {
        for profile in &mut projection.profiles {
            let home = &mut profile.home;
            if home.grants.is_empty()
                && matches!(
                    home.lot.source.as_ref(),
                    "fixture-room" | "legacy-preview-grid"
                )
            {
                let owner = home.owner_id.clone();
                for instance in &mut home.instances {
                    if instance.owner_id.is_none() {
                        instance.owner_id = Some(owner.clone());
                    }
                }
                home.grants = vec![LotGrant {
                    actor_id: owner.clone(),
                    purchase: Some(PurchaseGrant {
                        availability: home.permissions.purchase.clone(),
                        payer_id: owner.clone(),
                        object_owner_id: owner.clone(),
                        categories: None,
                    }),
                    arrange: home.permissions.arrange.clone(),
                    build: Availability::Unavailable {
                        reason: "The construction service has not been connected".into(),
                    },
                    inventory_owners: vec![owner],
                }];
            }
        }
    }
    projection
}
