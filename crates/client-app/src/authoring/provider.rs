use super::preview_authoring_projection;
use super::transition::after_outcome;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, Character, OperationId};

/// Disposable preview replay safety; neither value is a game/account limit.
pub const MAX_PREVIEW_OPERATIONS: usize = 256;
const MAX_PREVIEW_RECEIPT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
struct CachedReply {
    request: AuthoringRequest,
    event: AuthoringEvent,
}

/// A local simulation adapter. It does not connect an account, spend live money,
/// construct architecture, or claim that missing renderer assets are available.
#[derive(Debug)]
pub struct PreviewAuthoringProvider {
    projection: AuthoringProjection,
    replies: BTreeMap<OperationId, CachedReply>,
    retained_bytes: usize,
}

impl PreviewAuthoringProvider {
    pub fn new(projection: AuthoringProjection) -> Result<Self, AuthoringError> {
        projection.validate()?;
        Ok(Self {
            projection,
            replies: BTreeMap::new(),
            retained_bytes: 0,
        })
    }

    pub fn snapshot(&self) -> &AuthoringProjection {
        &self.projection
    }

    /// Install a validated, newer metadata snapshot without reopening operation
    /// identities or resetting retained replay budgets. The shell coordinates
    /// in-flight requests, save completion and the reducer replacement atomically.
    pub fn replace_projection(
        &mut self,
        projection: AuthoringProjection,
    ) -> Result<(), AuthoringError> {
        projection.validate()?;
        if projection.revision <= self.projection.revision {
            return Err(AuthoringError::StaleRevision);
        }
        self.projection = projection;
        Ok(())
    }

    pub fn handle(&mut self, request: &AuthoringRequest) -> AuthoringEvent {
        if let Some(cached) = self.replies.get(&request.operation_id) {
            return if &cached.request == request {
                cached.event.clone()
            } else {
                rejected(request, AuthoringError::InvalidOperation)
            };
        }
        if let Err(error) = request_bounds(request) {
            return rejected(request, error);
        }
        if self.replies.len() >= MAX_PREVIEW_OPERATIONS {
            return rejected(request, AuthoringError::OperationLimit);
        }
        let event = match self.commit(request) {
            Ok((projection, outcome)) => AuthoringEvent::Committed {
                operation_id: request.operation_id.clone(),
                base_revision: request.base_revision,
                projection,
                outcome,
            },
            Err(error) => rejected(request, error),
        };
        let bytes = serde_json::to_vec(&event)
            .map(|value| value.len())
            .unwrap_or(usize::MAX);
        let request_bytes = serde_json::to_vec(request)
            .map(|value| value.len())
            .unwrap_or(usize::MAX);
        let Some(retained_bytes) = self
            .retained_bytes
            .checked_add(bytes)
            .and_then(|value| value.checked_add(request_bytes))
        else {
            return rejected(request, AuthoringError::OperationLimit);
        };
        if retained_bytes > MAX_PREVIEW_RECEIPT_BYTES {
            return rejected(request, AuthoringError::OperationLimit);
        }
        if let AuthoringEvent::Committed { projection, .. } = &event {
            self.projection = projection.clone();
        }
        self.retained_bytes = retained_bytes;
        self.replies.insert(
            request.operation_id.clone(),
            CachedReply {
                request: request.clone(),
                event: event.clone(),
            },
        );
        event
    }

    fn commit(
        &self,
        request: &AuthoringRequest,
    ) -> Result<(AuthoringProjection, AuthoringOutcome), AuthoringError> {
        if request.base_revision != self.projection.revision
            || request.expected_sources != self.projection.source_revisions(&request.kind)
        {
            return Err(AuthoringError::StaleRevision);
        }
        let revision = self
            .projection
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::OperationLimit)?;
        let outcome = match &request.kind {
            AuthoringRequestKind::CreateProfile { .. } => AuthoringOutcome::ProfileCreated {
                character_id: unused_id(
                    "preview-profile",
                    revision,
                    &self
                        .projection
                        .homes()
                        .map(|home| home.owner_id.as_ref())
                        .collect(),
                )?
                .into(),
            },
            AuthoringRequestKind::SetOutfit {
                actor_id,
                owned_outfit_id,
                action,
            } => {
                let wardrobe = &self
                    .projection
                    .profile(actor_id)
                    .ok_or(AuthoringError::UnknownProfile)?
                    .wardrobe;
                let outfit = wardrobe
                    .outfit(owned_outfit_id)
                    .ok_or(AuthoringError::UnknownOutfit)?;
                let replacement_default = if *action == WardrobeAction::Delete && outfit.is_default
                {
                    wardrobe
                        .outfits
                        .iter()
                        .find(|owned| {
                            owned.id != outfit.id && owned.category_id == outfit.category_id
                        })
                        .map(|owned| owned.id.clone())
                } else {
                    None
                };
                AuthoringOutcome::OutfitSaved {
                    character_id: actor_id.clone(),
                    replacement_default,
                }
            }
            AuthoringRequestKind::BuyAndPlace { home_owner_id, .. } => {
                AuthoringOutcome::Purchased {
                    owner_id: home_owner_id.clone(),
                    instance_id: unused_id(
                        "preview-instance",
                        revision,
                        &self
                            .projection
                            .homes()
                            .flat_map(|home| {
                                home.instances.iter().map(|instance| instance.id.as_ref())
                            })
                            .collect(),
                    )?
                    .into(),
                }
            }
            AuthoringRequestKind::MoveInstance {
                home_owner_id,
                instance_id,
                ..
            } => AuthoringOutcome::Moved {
                owner_id: home_owner_id.clone(),
                instance_id: instance_id.clone(),
            },
            AuthoringRequestKind::StoreInstance {
                home_owner_id,
                instance_id,
                ..
            } => AuthoringOutcome::Stored {
                owner_id: home_owner_id.clone(),
                instance_id: instance_id.clone(),
            },
            AuthoringRequestKind::PlaceOwned {
                home_owner_id,
                instance_id,
                ..
            } => AuthoringOutcome::Placed {
                owner_id: home_owner_id.clone(),
                instance_id: instance_id.clone(),
            },
        };
        let created = match (&request.kind, &outcome) {
            (
                AuthoringRequestKind::CreateProfile {
                    name,
                    description,
                    shard_id,
                    appearance,
                },
                AuthoringOutcome::ProfileCreated { character_id },
            ) => {
                let sample = preview_authoring_projection().profiles.remove(0);
                let mut home = sample.home;
                home.owner_id = character_id.clone();
                for grant in &mut home.grants {
                    grant.actor_id = character_id.clone();
                    grant.inventory_owners = vec![character_id.clone()];
                    if let Some(purchase) = &mut grant.purchase {
                        purchase.payer_id = character_id.clone();
                        purchase.object_owner_id = character_id.clone();
                    }
                }
                Some(AuthoringProfile {
                    character: Character {
                        id: character_id.clone(),
                        name: name.clone(),
                        availability: Availability::Available,
                        money: 1_250,
                        needs: sample.character.needs,
                    },
                    description: description.clone(),
                    shard_id: shard_id.clone(),
                    appearance: appearance.clone(),
                    portrait: None,
                    wardrobe: Wardrobe {
                        revision: 1,
                        categories: vec![],
                        outfits: vec![],
                    },
                    home,
                })
            }
            _ => None,
        };
        let projection = after_outcome(
            &self.projection,
            &request.kind,
            &outcome,
            revision,
            created.as_ref(),
        )?;
        Ok((projection, outcome))
    }
}

fn unused_id(prefix: &str, start: u64, used: &BTreeSet<&str>) -> Result<String, AuthoringError> {
    let mut sequence = start;
    for _ in 0..=used.len() {
        let id = format!("{prefix}-{sequence}");
        if !used.contains(id.as_str()) {
            return Ok(id);
        }
        sequence = sequence
            .checked_add(1)
            .ok_or(AuthoringError::OperationLimit)?;
    }
    Err(AuthoringError::OperationLimit)
}

fn rejected(request: &AuthoringRequest, error: AuthoringError) -> AuthoringEvent {
    AuthoringEvent::Rejected {
        operation_id: request.operation_id.clone(),
        base_revision: request.base_revision,
        error,
    }
}

/// Bound everything retained by replay handling even for a malformed caller.
fn request_bounds(request: &AuthoringRequest) -> Result<(), AuthoringError> {
    if !is_valid_authoring_id(request.operation_id.as_ref()) {
        return Err(AuthoringError::InvalidOperation);
    }
    let mut ids = Vec::new();
    match &request.kind {
        AuthoringRequestKind::CreateProfile {
            name,
            description,
            shard_id,
            appearance,
        } => {
            normalize_profile_name(name)?;
            if name.len() > 128
                || description.len() > MAX_DESCRIPTION_BYTES
                || appearance.decorations.len() > 256
            {
                return Err(AuthoringError::SafetyLimit);
            }
            if let Some(id) = shard_id {
                ids.push(id.as_ref());
            }
            for key in [
                &appearance.head,
                &appearance.body,
                &appearance.skin_tone,
                &appearance.gender,
            ]
            .into_iter()
            .flatten()
            {
                ids.push(key.as_ref());
            }
            for (category, key) in &appearance.decorations {
                ids.push(category.as_ref());
                ids.push(key.as_ref());
            }
        }
        AuthoringRequestKind::SetOutfit {
            actor_id,
            owned_outfit_id,
            ..
        } => {
            ids.extend([actor_id.as_ref(), owned_outfit_id.as_ref()]);
        }
        AuthoringRequestKind::BuyAndPlace {
            actor_id,
            home_owner_id,
            catalog_id,
            ..
        } => {
            ids.extend([
                actor_id.as_ref(),
                home_owner_id.as_ref(),
                catalog_id.as_ref(),
            ]);
        }
        AuthoringRequestKind::MoveInstance {
            actor_id,
            home_owner_id,
            instance_id,
            ..
        }
        | AuthoringRequestKind::StoreInstance {
            actor_id,
            home_owner_id,
            instance_id,
        }
        | AuthoringRequestKind::PlaceOwned {
            actor_id,
            home_owner_id,
            instance_id,
            ..
        } => {
            ids.extend([
                actor_id.as_ref(),
                home_owner_id.as_ref(),
                instance_id.as_ref(),
            ]);
        }
    }
    if ids.into_iter().any(|id| !is_valid_authoring_id(id)) {
        return Err(AuthoringError::InvalidOperation);
    }
    Ok(())
}
