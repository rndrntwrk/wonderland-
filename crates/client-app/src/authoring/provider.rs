use super::transition::after_outcome;
use std::collections::BTreeMap;
use wonderland_contracts::OperationId;
use wonderland_contracts::authoring::*;

/// Bounded replay receipts for a single disposable UI preview session.
pub const MAX_PREVIEW_OPERATIONS: usize = 256;

#[derive(Debug)]
struct CachedReply {
    request: AuthoringRequest,
    event: AuthoringEvent,
}

/// Pure fixture/demo provider. It owns only a validated local preview snapshot.
/// No network, timers, authentication, renderer, live money, or canonical placement.
#[derive(Debug)]
pub struct PreviewAuthoringProvider {
    projection: AuthoringProjection,
    replies: BTreeMap<OperationId, CachedReply>,
}

impl PreviewAuthoringProvider {
    pub fn new(projection: AuthoringProjection) -> Result<Self, AuthoringError> {
        projection.validate()?;
        Ok(Self {
            projection,
            replies: BTreeMap::new(),
        })
    }

    pub fn snapshot(&self) -> &AuthoringProjection {
        &self.projection
    }

    /// Recheck against the provider snapshot. Exact repeats replay their receipt;
    /// reusing an operation for another request is rejected without mutation.
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
        let result = self.commit(request);
        let event = match result {
            Ok((projection, outcome)) => {
                self.projection = projection.clone();
                AuthoringEvent::Committed {
                    operation_id: request.operation_id.clone(),
                    base_revision: request.base_revision,
                    projection,
                    outcome,
                }
            }
            Err(error) => rejected(request, error),
        };
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
        if request.base_revision != self.projection.revision {
            return Err(AuthoringError::StaleRevision);
        }
        let revision = self
            .projection
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::OperationLimit)?;
        let outcome = match &request.kind {
            AuthoringRequestKind::CreateProfile { .. } => AuthoringOutcome::ProfileCreated {
                character_id: self
                    .unused_id("preview-profile", revision, |id| {
                        self.projection
                            .profiles
                            .iter()
                            .any(|profile| profile.character.id.as_ref() == id)
                    })?
                    .into(),
            },
            AuthoringRequestKind::SetOutfit { actor_id, .. } => AuthoringOutcome::OutfitSaved {
                character_id: actor_id.clone(),
            },
            AuthoringRequestKind::BuyAndPlace { home_owner_id, .. } => {
                AuthoringOutcome::Purchased {
                    owner_id: home_owner_id.clone(),
                    instance_id: self
                        .unused_id("preview-instance", revision, |id| {
                            self.projection.profiles.iter().any(|profile| {
                                profile
                                    .home
                                    .instances
                                    .iter()
                                    .any(|instance| instance.id.as_ref() == id)
                            })
                        })?
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
        let projection = after_outcome(&self.projection, &request.kind, &outcome, revision)?;
        Ok((projection, outcome))
    }

    fn unused_id(
        &self,
        prefix: &str,
        start: u64,
        used: impl Fn(&str) -> bool,
    ) -> Result<String, AuthoringError> {
        let mut sequence = start;
        // A validated snapshot can contain at most this many distinct instances.
        for _ in 0..=MAX_PROFILES * MAX_OWNED_INSTANCES {
            let id = format!("{prefix}-{sequence}");
            if !used(&id) {
                return Ok(id);
            }
            sequence = sequence
                .checked_add(1)
                .ok_or(AuthoringError::OperationLimit)?;
        }
        Err(AuthoringError::OperationLimit)
    }
}

fn rejected(request: &AuthoringRequest, error: AuthoringError) -> AuthoringEvent {
    AuthoringEvent::Rejected {
        operation_id: request.operation_id.clone(),
        base_revision: request.base_revision,
        error,
    }
}

/// Bound what is retained by the receipt cache even for a malformed caller.
fn request_bounds(request: &AuthoringRequest) -> Result<(), AuthoringError> {
    if !is_valid_authoring_id(request.operation_id.as_ref()) {
        return Err(AuthoringError::InvalidOperation);
    }
    let ids: Vec<&str> = match &request.kind {
        AuthoringRequestKind::CreateProfile { name, look_id, .. } => {
            normalize_profile_name(name)?;
            if name.len() > 128 {
                return Err(AuthoringError::InvalidName(
                    "Submit a trimmed name of at most 128 UTF-8 bytes".into(),
                ));
            }
            vec![look_id.as_ref()]
        }
        AuthoringRequestKind::SetOutfit { actor_id, look_id } => {
            vec![actor_id.as_ref(), look_id.as_ref()]
        }
        AuthoringRequestKind::BuyAndPlace {
            actor_id,
            home_owner_id,
            catalog_id,
            ..
        } => vec![
            actor_id.as_ref(),
            home_owner_id.as_ref(),
            catalog_id.as_ref(),
        ],
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
        } => vec![
            actor_id.as_ref(),
            home_owner_id.as_ref(),
            instance_id.as_ref(),
        ],
    };
    if ids.into_iter().any(|id| !is_valid_authoring_id(id)) {
        return Err(AuthoringError::InvalidOperation);
    }
    Ok(())
}
