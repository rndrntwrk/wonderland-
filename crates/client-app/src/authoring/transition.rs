//! Shared rules for this bounded fixture and strict receipt delta validation.
//! These rules are not a simulation or a replacement for a live authority adapter.

use super::preview_authoring_projection;
use std::collections::BTreeSet;
use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, CharacterId};

pub(super) fn ensure_available(value: &Availability) -> Result<(), AuthoringError> {
    match value {
        Availability::Available => Ok(()),
        Availability::Unavailable { reason } => Err(AuthoringError::Unavailable(reason.clone())),
    }
}

fn actor<'a>(
    projection: &'a AuthoringProjection,
    id: &CharacterId,
) -> Result<&'a AuthoringProfile, AuthoringError> {
    let profile = projection
        .profile(id)
        .ok_or(AuthoringError::UnknownProfile)?;
    ensure_available(&profile.character.availability)?;
    Ok(profile)
}

fn owned_home<'a>(
    projection: &'a AuthoringProjection,
    actor_id: &CharacterId,
    owner_id: &CharacterId,
    purchase: bool,
) -> Result<&'a AuthoringProfile, AuthoringError> {
    let profile = actor(projection, actor_id)?;
    projection
        .home(owner_id)
        .ok_or(AuthoringError::UnknownProfile)?;
    if actor_id != owner_id {
        return Err(AuthoringError::PermissionDenied);
    }
    ensure_available(if purchase {
        &profile.home.permissions.purchase
    } else {
        &profile.home.permissions.arrange
    })?;
    Ok(profile)
}

fn placement(
    projection: &AuthoringProjection,
    home: &Home,
    item: &CatalogItem,
    pose: GridPose,
    ignore_instance: Option<&OwnedInstanceId>,
) -> Result<(), AuthoringError> {
    let candidate: BTreeSet<_> = item.footprint.cells(pose)?.into_iter().collect();
    for instance in &home.instances {
        if ignore_instance == Some(&instance.id) {
            continue;
        }
        if let Some(placed) = instance.placement {
            let occupied_item = projection
                .catalog_item(&instance.catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            if occupied_item
                .footprint
                .cells(placed)?
                .iter()
                .any(|cell| candidate.contains(cell))
            {
                return Err(AuthoringError::Occupied);
            }
        }
    }
    Ok(())
}

/// Preflight is advisory in the reducer and repeated against the provider snapshot.
pub(super) fn validate_kind(
    projection: &AuthoringProjection,
    kind: &AuthoringRequestKind,
) -> Result<(), AuthoringError> {
    projection.validate()?;
    match kind {
        AuthoringRequestKind::CreateProfile {
            name,
            identity,
            look_id,
        } => {
            let normalized = normalize_profile_name(name)?;
            if &normalized != name {
                return Err(AuthoringError::InvalidName("Submit a trimmed name".into()));
            }
            if look_id.style_for(*identity).is_none() {
                return Err(AuthoringError::InvalidLook);
            }
            if projection.profiles.len() >= MAX_PROFILES {
                return Err(AuthoringError::ProfileLimit);
            }
        }
        AuthoringRequestKind::SetOutfit { actor_id, look_id } => {
            let profile = actor(projection, actor_id)?;
            if look_id.style_for(profile.identity).is_none() {
                return Err(AuthoringError::InvalidLook);
            }
        }
        AuthoringRequestKind::BuyAndPlace {
            actor_id,
            home_owner_id,
            catalog_id,
            pose,
        } => {
            let profile = owned_home(projection, actor_id, home_owner_id, true)?;
            let item = projection
                .catalog_item(catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            ensure_available(&item.availability)?;
            if profile.character.money < item.price {
                return Err(AuthoringError::InsufficientFunds);
            }
            if profile.home.instances.len() >= MAX_OWNED_INSTANCES {
                return Err(AuthoringError::InventoryLimit);
            }
            placement(projection, &profile.home, item, *pose, None)?;
        }
        AuthoringRequestKind::MoveInstance {
            actor_id,
            home_owner_id,
            instance_id,
            pose,
        }
        | AuthoringRequestKind::PlaceOwned {
            actor_id,
            home_owner_id,
            instance_id,
            pose,
        } => {
            let profile = owned_home(projection, actor_id, home_owner_id, false)?;
            let instance = profile
                .home
                .instance(instance_id)
                .ok_or(AuthoringError::UnknownInstance)?;
            let moving = matches!(kind, AuthoringRequestKind::MoveInstance { .. });
            if instance.placement.is_some() != moving {
                return Err(AuthoringError::WrongPlacementState);
            }
            let item = projection
                .catalog_item(&instance.catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            placement(
                projection,
                &profile.home,
                item,
                *pose,
                moving.then_some(instance_id),
            )?;
        }
        AuthoringRequestKind::StoreInstance {
            actor_id,
            home_owner_id,
            instance_id,
        } => {
            let profile = owned_home(projection, actor_id, home_owner_id, false)?;
            let instance = profile
                .home
                .instance(instance_id)
                .ok_or(AuthoringError::UnknownInstance)?;
            if instance.placement.is_none() {
                return Err(AuthoringError::WrongPlacementState);
            }
        }
    }
    Ok(())
}

fn profile_mut<'a>(
    projection: &'a mut AuthoringProjection,
    id: &CharacterId,
) -> Result<&'a mut AuthoringProfile, AuthoringError> {
    projection
        .profiles
        .iter_mut()
        .find(|profile| &profile.character.id == id)
        .ok_or(AuthoringError::UnknownProfile)
}

fn instance_mut<'a>(
    projection: &'a mut AuthoringProjection,
    owner_id: &CharacterId,
    id: &OwnedInstanceId,
) -> Result<&'a mut OwnedInstance, AuthoringError> {
    profile_mut(projection, owner_id)?
        .home
        .instances
        .iter_mut()
        .find(|instance| &instance.id == id)
        .ok_or(AuthoringError::UnknownInstance)
}

/// Build an exact permitted delta in a copy. The caller still validates its receipt
/// against this result before swapping any committed state.
pub(super) fn after_outcome(
    projection: &AuthoringProjection,
    kind: &AuthoringRequestKind,
    outcome: &AuthoringOutcome,
    revision: u64,
) -> Result<AuthoringProjection, AuthoringError> {
    validate_kind(projection, kind)?;
    if revision <= projection.revision {
        return Err(AuthoringError::StaleRevision);
    }
    let mut next = projection.clone();
    match (kind, outcome) {
        (
            AuthoringRequestKind::CreateProfile {
                name,
                identity,
                look_id,
            },
            AuthoringOutcome::ProfileCreated { character_id },
        ) => {
            if !is_valid_authoring_id(character_id.as_ref())
                || projection.profile(character_id).is_some()
            {
                return Err(AuthoringError::InvalidOutcome);
            }
            let mut profile = preview_authoring_projection()
                .profiles
                .into_iter()
                .find(|profile| profile.identity == *identity)
                .ok_or(AuthoringError::InvalidLook)?;
            profile.character.id = character_id.clone();
            profile.character.name = name.clone();
            profile.look_id = look_id.clone();
            profile.home.owner_id = character_id.clone();
            next.profiles.push(profile);
        }
        (
            AuthoringRequestKind::SetOutfit { actor_id, look_id },
            AuthoringOutcome::OutfitSaved { character_id },
        ) if actor_id == character_id => {
            profile_mut(&mut next, actor_id)?.look_id = look_id.clone();
        }
        (
            AuthoringRequestKind::BuyAndPlace {
                home_owner_id,
                catalog_id,
                pose,
                ..
            },
            AuthoringOutcome::Purchased {
                owner_id,
                instance_id,
            },
        ) if home_owner_id == owner_id => {
            if !is_valid_authoring_id(instance_id.as_ref())
                || projection
                    .profiles
                    .iter()
                    .any(|profile| profile.home.instance(instance_id).is_some())
            {
                return Err(AuthoringError::InvalidOutcome);
            }
            let price = projection
                .catalog_item(catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?
                .price;
            let profile = profile_mut(&mut next, owner_id)?;
            profile.character.money = profile
                .character
                .money
                .checked_sub(price)
                .ok_or(AuthoringError::InsufficientFunds)?;
            profile.home.instances.push(OwnedInstance {
                id: instance_id.clone(),
                catalog_id: catalog_id.clone(),
                placement: Some(*pose),
            });
        }
        (
            AuthoringRequestKind::MoveInstance {
                home_owner_id,
                instance_id,
                pose,
                ..
            },
            AuthoringOutcome::Moved {
                owner_id,
                instance_id: changed,
            },
        )
        | (
            AuthoringRequestKind::PlaceOwned {
                home_owner_id,
                instance_id,
                pose,
                ..
            },
            AuthoringOutcome::Placed {
                owner_id,
                instance_id: changed,
            },
        ) if home_owner_id == owner_id && instance_id == changed => {
            instance_mut(&mut next, owner_id, instance_id)?.placement = Some(*pose);
        }
        (
            AuthoringRequestKind::StoreInstance {
                home_owner_id,
                instance_id,
                ..
            },
            AuthoringOutcome::Stored {
                owner_id,
                instance_id: changed,
            },
        ) if home_owner_id == owner_id && instance_id == changed => {
            instance_mut(&mut next, owner_id, instance_id)?.placement = None;
        }
        _ => return Err(AuthoringError::InvalidOutcome),
    }
    next.revision = revision;
    next.validate()?;
    Ok(next)
}
