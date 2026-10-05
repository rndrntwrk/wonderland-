//! Shared request checks and strict receipt delta validation.
//! These rules are not a simulation or a replacement for a live authority adapter.

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
    if !item.rotations.contains(&pose.direction) {
        return Err(AuthoringError::Unavailable("This item does not support the requested orientation".into()));
    }
    let candidate: BTreeSet<_> = item.footprint.cells(pose, &home.lot)?.into_iter().collect();
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
                .cells(placed, &home.lot)?
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
            name, description, shard_id, appearance,
        } => {
            projection.creation_allowed()?;
            projection.account.fields.validate_fields(name, description)?;
            projection.account.validate_shard(shard_id)?;
            projection.appearance_content.validate_selection(appearance)?;
        }
        AuthoringRequestKind::SetOutfit { actor_id, owned_outfit_id, action } => {
            let profile = actor(projection, actor_id)?;
            let outfit = profile.wardrobe.outfit(owned_outfit_id)
                .ok_or(AuthoringError::UnknownOutfit)?;
            let offer = outfit.actions.iter().find(|offer| offer.action == *action)
                .ok_or_else(|| AuthoringError::Unavailable("This wardrobe action is not supplied".into()))?;
            ensure_available(&offer.availability)?;
            if *action == WardrobeAction::Change {
                let category = profile.wardrobe.categories.iter().find(|category| category.id == outfit.category_id)
                    .ok_or(AuthoringError::UnknownOutfit)?;
                let options = match category.slot {
                    AppearanceSlot::Head => &projection.appearance_content.heads,
                    AppearanceSlot::Body => &projection.appearance_content.bodies,
                    AppearanceSlot::Decoration => return Ok(()),
                };
                if options.iter().find(|option| option.key == outfit.content_key)
                    .is_some_and(|option| !option.compatible(&profile.appearance))
                {
                    return Err(AuthoringError::InvalidAppearance("This owned outfit is incompatible with the current gender or skin tone".into()));
                }
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
            match profile.home.instance_capacity.allows_addition(profile.home.instances.len()) {
                Some(true) => {},
                Some(false) => return Err(AuthoringError::InventoryLimit),
                None => return Err(AuthoringError::Unavailable("The lot service has not supplied ownership capacity".into())),
            }
            if projection.profiles.iter().map(|profile| profile.home.instances.len()).sum::<usize>() >= MAX_OWNED_RECORDS {
                return Err(AuthoringError::SafetyLimit);
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
    created_profile: Option<&AuthoringProfile>,
) -> Result<AuthoringProjection, AuthoringError> {
    validate_kind(projection, kind)?;
    if revision <= projection.revision {
        return Err(AuthoringError::StaleRevision);
    }
    let mut next = projection.clone();
    match (kind, outcome) {
        (
            AuthoringRequestKind::CreateProfile {
                name, description, shard_id, appearance,
            },
            AuthoringOutcome::ProfileCreated { character_id },
        ) => {
            if !is_valid_authoring_id(character_id.as_ref()) || projection.profile(character_id).is_some() {
                return Err(AuthoringError::InvalidOutcome);
            }
            let profile = created_profile.ok_or(AuthoringError::InvalidOutcome)?;
            if profile.character.id != *character_id
                || profile.character.name != *name
                || profile.description != *description
                || profile.shard_id != *shard_id
                || profile.appearance != *appearance
                || profile.home.owner_id != *character_id
                || profile.portrait.is_some()
            {
                return Err(AuthoringError::InvalidOutcome);
            }
            // Initial money, needs and lot data come from the accepting service.
            // Only this new profile may differ; the complete delta is compared.
            next.profiles.push(profile.clone());
        }
        (
            AuthoringRequestKind::SetOutfit { actor_id, owned_outfit_id, action },
            AuthoringOutcome::OutfitSaved { character_id },
        ) if actor_id == character_id => {
            let profile = profile_mut(&mut next, actor_id)?;
            let outfit = profile.wardrobe.outfit(owned_outfit_id)
                .ok_or(AuthoringError::UnknownOutfit)?.clone();
            let category = profile.wardrobe.categories.iter().find(|category| category.id == outfit.category_id)
                .ok_or(AuthoringError::UnknownOutfit)?.clone();
            match action {
                WardrobeAction::Change => match category.slot {
                    AppearanceSlot::Head => profile.appearance.head = Some(outfit.content_key),
                    AppearanceSlot::Body => profile.appearance.body = Some(outfit.content_key),
                    AppearanceSlot::Decoration => {
                        profile.appearance.decorations.insert(category.id, outfit.content_key);
                    }
                },
                WardrobeAction::SetDefault => {
                    for owned in &mut profile.wardrobe.outfits {
                        if owned.category_id == outfit.category_id {
                            owned.is_default = owned.id == *owned_outfit_id;
                        }
                    }
                }
                WardrobeAction::Delete => {
                    profile.wardrobe.outfits.retain(|owned| &owned.id != owned_outfit_id);
                }
            }
            profile.wardrobe.revision = profile.wardrobe.revision.checked_add(1)
                .ok_or(AuthoringError::OperationLimit)?;
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
    let changed_home = match outcome {
        AuthoringOutcome::Purchased { owner_id, .. }
        | AuthoringOutcome::Moved { owner_id, .. }
        | AuthoringOutcome::Stored { owner_id, .. }
        | AuthoringOutcome::Placed { owner_id, .. } => Some(owner_id),
        _ => None,
    };
    if let Some(owner) = changed_home {
        let home = &mut profile_mut(&mut next, owner)?.home;
        home.lot.revision = home.lot.revision.checked_add(1).ok_or(AuthoringError::OperationLimit)?;
    }
    next.revision = revision;
    next.validate()?;
    Ok(next)
}
