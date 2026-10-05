//! Shared request checks and strict receipt delta validation.
//! These rules are not a simulation or a replacement for a live authority adapter.

use std::collections::{BTreeMap, BTreeSet};
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

fn authorized_home<'a>(
    projection: &'a AuthoringProjection,
    actor_id: &CharacterId,
    owner_id: &CharacterId,
    purchase: bool,
) -> Result<(&'a Home, &'a LotGrant), AuthoringError> {
    actor(projection, actor_id)?;
    let home = projection
        .home(owner_id)
        .ok_or(AuthoringError::UnknownProfile)?;
    let grant = home
        .grant(actor_id)
        .ok_or(AuthoringError::PermissionDenied)?;
    if purchase {
        ensure_available(&home.permissions.purchase)?;
        let purchase = grant
            .purchase
            .as_ref()
            .ok_or(AuthoringError::PermissionDenied)?;
        ensure_available(&purchase.availability)?;
    } else {
        ensure_available(&home.permissions.arrange)?;
        ensure_available(&grant.arrange)?;
    }
    Ok((home, grant))
}

fn capacity_for_addition(home: &Home) -> Result<(), AuthoringError> {
    match home.instance_capacity.allows_addition(home.instances.len()) {
        Some(true) => Ok(()),
        Some(false) => Err(AuthoringError::InventoryLimit),
        None => Err(AuthoringError::Unavailable(
            "The lot service has not supplied ownership capacity".into(),
        )),
    }
}

fn placement(
    projection: &AuthoringProjection,
    home: &Home,
    item: &CatalogItem,
    pose: GridPose,
    ignore_instance: Option<&OwnedInstanceId>,
) -> Result<(), AuthoringError> {
    if !item.rotations.contains(&pose.direction) {
        return Err(AuthoringError::Unavailable(
            "This item does not support the requested orientation".into(),
        ));
    }
    let lot_index = home.lot.index()?;
    let candidate: BTreeSet<_> = lot_index.cells(item.footprint, pose)?.into_iter().collect();
    let catalog: BTreeMap<_, _> = projection
        .catalog
        .iter()
        .map(|item| (&item.id, item))
        .collect();
    for instance in &home.instances {
        if ignore_instance == Some(&instance.id) {
            continue;
        }
        if let Some(placed) = instance.placement {
            let occupied_item = catalog
                .get(&instance.catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            if lot_index
                .cells(occupied_item.footprint, placed)?
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
            description,
            shard_id,
            appearance,
        } => {
            projection.creation_allowed()?;
            projection
                .account
                .fields
                .validate_fields(name, description)?;
            projection.account.validate_shard(shard_id)?;
            projection
                .appearance_content
                .validate_selection(appearance)?;
        }
        AuthoringRequestKind::SetOutfit {
            actor_id,
            owned_outfit_id,
            action,
        } => {
            let profile = actor(projection, actor_id)?;
            let outfit = profile
                .wardrobe
                .outfit(owned_outfit_id)
                .ok_or(AuthoringError::UnknownOutfit)?;
            let offer = outfit
                .actions
                .iter()
                .find(|offer| offer.action == *action)
                .ok_or_else(|| {
                    AuthoringError::Unavailable("This wardrobe action is not supplied".into())
                })?;
            ensure_available(&offer.availability)?;
            if *action == WardrobeAction::Delete
                && outfit.is_default
                && !profile
                    .wardrobe
                    .outfits
                    .iter()
                    .any(|owned| owned.id != outfit.id && owned.category_id == outfit.category_id)
            {
                return Err(AuthoringError::Unavailable(
                    "A category's default needs a surviving outfit before it can be deleted".into(),
                ));
            }
            if *action == WardrobeAction::Change {
                let category = profile
                    .wardrobe
                    .categories
                    .iter()
                    .find(|category| category.id == outfit.category_id)
                    .ok_or(AuthoringError::UnknownOutfit)?;
                let options = match category.slot {
                    AppearanceSlot::Head => &projection.appearance_content.heads,
                    AppearanceSlot::Body => &projection.appearance_content.bodies,
                    AppearanceSlot::Decoration => return Ok(()),
                };
                if options
                    .iter()
                    .find(|option| option.key == outfit.content_key)
                    .is_some_and(|option| !option.compatible(&profile.appearance))
                {
                    return Err(AuthoringError::InvalidAppearance(
                        "This owned outfit is incompatible with the current gender or skin tone"
                            .into(),
                    ));
                }
            }
        }
        AuthoringRequestKind::BuyAndPlace {
            actor_id,
            home_owner_id,
            catalog_id,
            pose,
        } => {
            let (home, grant) = authorized_home(projection, actor_id, home_owner_id, true)?;
            let purchase = grant
                .purchase
                .as_ref()
                .ok_or(AuthoringError::PermissionDenied)?;
            let item = projection
                .catalog_item(catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            ensure_available(&item.availability)?;
            if purchase
                .categories
                .as_ref()
                .is_some_and(|categories| !categories.contains(&item.category))
            {
                return Err(AuthoringError::PermissionDenied);
            }
            let payer = projection.profile(&purchase.payer_id).ok_or_else(|| {
                AuthoringError::Unavailable("The purchase payer's balance is not supplied".into())
            })?;
            if payer.character.money < item.price {
                return Err(AuthoringError::InsufficientFunds);
            }
            capacity_for_addition(home)?;
            if projection
                .homes()
                .map(|home| home.instances.len())
                .sum::<usize>()
                >= MAX_OWNED_RECORDS
            {
                return Err(AuthoringError::SafetyLimit);
            }
            placement(projection, home, item, *pose, None)?;
        }
        AuthoringRequestKind::MoveInstance {
            actor_id,
            home_owner_id,
            instance_id,
            pose,
        } => {
            let (home, _) = authorized_home(projection, actor_id, home_owner_id, false)?;
            let instance = home
                .instance(instance_id)
                .ok_or(AuthoringError::UnknownInstance)?;
            if instance.placement.is_none() {
                return Err(AuthoringError::WrongPlacementState);
            }
            let item = projection
                .catalog_item(&instance.catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            placement(projection, home, item, *pose, Some(instance_id))?;
        }
        AuthoringRequestKind::PlaceOwned {
            actor_id,
            home_owner_id,
            instance_id,
            pose,
        } => {
            let (home, grant) = authorized_home(projection, actor_id, home_owner_id, false)?;
            let (source, instance) = projection
                .owned_instance(instance_id)
                .ok_or(AuthoringError::UnknownInstance)?;
            if instance.placement.is_some() {
                return Err(AuthoringError::WrongPlacementState);
            }
            if !instance
                .owner_id
                .as_ref()
                .is_some_and(|owner| grant.inventory_owners.contains(owner))
            {
                return Err(AuthoringError::PermissionDenied);
            }
            if source.owner_id != home.owner_id {
                capacity_for_addition(home)?;
            }
            let item = projection
                .catalog_item(&instance.catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?;
            placement(projection, home, item, *pose, None)?;
        }

        AuthoringRequestKind::StoreInstance {
            actor_id,
            home_owner_id,
            instance_id,
        } => {
            let (home, _) = authorized_home(projection, actor_id, home_owner_id, false)?;
            let instance = home
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

fn home_mut<'a>(
    projection: &'a mut AuthoringProjection,
    owner_id: &CharacterId,
) -> Result<&'a mut Home, AuthoringError> {
    projection
        .profiles
        .iter_mut()
        .map(|profile| &mut profile.home)
        .chain(&mut projection.shared_homes)
        .find(|home| &home.owner_id == owner_id)
        .ok_or(AuthoringError::UnknownProfile)
}

fn instance_mut<'a>(
    projection: &'a mut AuthoringProjection,
    owner_id: &CharacterId,
    id: &OwnedInstanceId,
) -> Result<&'a mut OwnedInstance, AuthoringError> {
    home_mut(projection, owner_id)?
        .instances
        .iter_mut()
        .find(|instance| &instance.id == id)
        .ok_or(AuthoringError::UnknownInstance)
}

fn advance_lot(home: &mut Home) -> Result<(), AuthoringError> {
    home.lot.revision = home
        .lot
        .revision
        .checked_add(1)
        .ok_or(AuthoringError::OperationLimit)?;
    Ok(())
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
                name,
                description,
                shard_id,
                appearance,
            },
            AuthoringOutcome::ProfileCreated { character_id },
        ) => {
            if !is_valid_authoring_id(character_id.as_ref())
                || projection.profile(character_id).is_some()
            {
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
            AuthoringRequestKind::SetOutfit {
                actor_id,
                owned_outfit_id,
                action,
            },
            AuthoringOutcome::OutfitSaved {
                character_id,
                replacement_default,
            },
        ) if actor_id == character_id => {
            let profile = profile_mut(&mut next, actor_id)?;
            let outfit = profile
                .wardrobe
                .outfit(owned_outfit_id)
                .ok_or(AuthoringError::UnknownOutfit)?
                .clone();
            if *action == WardrobeAction::Delete && outfit.is_default {
                let replacement = replacement_default
                    .as_ref()
                    .and_then(|id| profile.wardrobe.outfit(id))
                    .ok_or(AuthoringError::InvalidOutcome)?;
                if replacement.id == outfit.id || replacement.category_id != outfit.category_id {
                    return Err(AuthoringError::InvalidOutcome);
                }
            } else if replacement_default.is_some() {
                return Err(AuthoringError::InvalidOutcome);
            }
            let category = profile
                .wardrobe
                .categories
                .iter()
                .find(|category| category.id == outfit.category_id)
                .ok_or(AuthoringError::UnknownOutfit)?
                .clone();
            match action {
                WardrobeAction::Change => match category.slot {
                    AppearanceSlot::Head => profile.appearance.head = Some(outfit.content_key),
                    AppearanceSlot::Body => profile.appearance.body = Some(outfit.content_key),
                    AppearanceSlot::Decoration => {
                        profile
                            .appearance
                            .decorations
                            .insert(category.id, outfit.content_key);
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
                    profile
                        .wardrobe
                        .outfits
                        .retain(|owned| &owned.id != owned_outfit_id);
                    if let Some(replacement) = replacement_default {
                        for owned in &mut profile.wardrobe.outfits {
                            if owned.category_id == outfit.category_id {
                                owned.is_default = &owned.id == replacement;
                            }
                        }
                    }
                }
            }
            profile.wardrobe.revision = profile
                .wardrobe
                .revision
                .checked_add(1)
                .ok_or(AuthoringError::OperationLimit)?;
        }
        (
            AuthoringRequestKind::BuyAndPlace {
                actor_id,
                home_owner_id,
                catalog_id,
                pose,
            },
            AuthoringOutcome::Purchased {
                owner_id,
                instance_id,
            },
        ) if home_owner_id == owner_id => {
            if !is_valid_authoring_id(instance_id.as_ref())
                || projection.owned_instance(instance_id).is_some()
            {
                return Err(AuthoringError::InvalidOutcome);
            }
            let price = projection
                .catalog_item(catalog_id)
                .ok_or(AuthoringError::UnknownCatalogItem)?
                .price;
            let (_, grant) = authorized_home(projection, actor_id, home_owner_id, true)?;
            let purchase = grant
                .purchase
                .as_ref()
                .ok_or(AuthoringError::PermissionDenied)?;
            let payer = profile_mut(&mut next, &purchase.payer_id)?;
            payer.character.money = payer
                .character
                .money
                .checked_sub(price)
                .ok_or(AuthoringError::InsufficientFunds)?;
            home_mut(&mut next, owner_id)?
                .instances
                .push(OwnedInstance {
                    id: instance_id.clone(),
                    catalog_id: catalog_id.clone(),
                    owner_id: Some(purchase.object_owner_id.clone()),
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
        ) if home_owner_id == owner_id && instance_id == changed => {
            instance_mut(&mut next, owner_id, instance_id)?.placement = Some(*pose);
        }
        (
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
            let (source, _) = projection
                .owned_instance(instance_id)
                .ok_or(AuthoringError::UnknownInstance)?;
            if &source.owner_id == owner_id {
                instance_mut(&mut next, owner_id, instance_id)?.placement = Some(*pose);
            } else {
                let source = home_mut(&mut next, &source.owner_id)?;
                let index = source
                    .instances
                    .iter()
                    .position(|instance| &instance.id == instance_id)
                    .ok_or(AuthoringError::UnknownInstance)?;
                let mut instance = source.instances.remove(index);
                advance_lot(source)?;
                instance.placement = Some(*pose);
                home_mut(&mut next, owner_id)?.instances.push(instance);
            }
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
        advance_lot(home_mut(&mut next, owner)?)?;
    }
    next.revision = revision;
    next.validate()?;
    Ok(next)
}
