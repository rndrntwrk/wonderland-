use super::transition::{after_outcome, ensure_available, validate_kind};
use wonderland_contracts::CharacterId;
use wonderland_contracts::authoring::*;

/// DOM-free presentation state. All fields are private; views use shared getters.
/// Operation IDs are unique for this reducer's lifetime, including replacements.
#[derive(Clone, Debug)]
pub struct AuthoringState {
    projection: AuthoringProjection,
    selected_profile: Option<CharacterId>,
    selected_home: Option<CharacterId>,
    selected_instance: Option<OwnedInstanceId>,
    draft: Option<AuthoringDraft>,
    pending: Option<AuthoringRequest>,
    last_error: Option<AuthoringError>,
    last_commit: Option<AuthoringCommit>,
    next_operation: u64,
}

impl AuthoringState {
    /// Invalid initial data remains readable with an error and disabled dispatch.
    /// A valid newer `ProjectionReplaced` event can recover it.
    pub fn new(projection: AuthoringProjection) -> Self {
        let last_error = projection.validate().err();
        Self {
            projection,
            selected_profile: None,
            selected_home: None,
            selected_instance: None,
            draft: None,
            pending: None,
            last_error,
            last_commit: None,
            next_operation: 1,
        }
    }

    pub fn projection(&self) -> &AuthoringProjection {
        &self.projection
    }
    pub fn selected_profile(&self) -> Option<&CharacterId> {
        self.selected_profile.as_ref()
    }
    pub fn selected_home(&self) -> Option<&CharacterId> {
        self.selected_home.as_ref()
    }
    pub fn selected_instance(&self) -> Option<&OwnedInstanceId> {
        self.selected_instance.as_ref()
    }
    pub fn draft(&self) -> Option<&AuthoringDraft> {
        self.draft.as_ref()
    }
    pub fn pending(&self) -> Option<&AuthoringRequest> {
        self.pending.as_ref()
    }
    pub fn last_error(&self) -> Option<&AuthoringError> {
        self.last_error.as_ref()
    }
    pub fn last_commit(&self) -> Option<&AuthoringCommit> {
        self.last_commit.as_ref()
    }

    /// Advisory validity for visible draft feedback. Submission and the provider
    /// both recheck it; an invalid candidate remains editable and is never placed.
    pub fn draft_validity(&self) -> Result<(), AuthoringError> {
        validate_kind(&self.projection, &self.draft_request_kind()?)
    }

    /// Draft edits never modify the committed snapshot. At most one mutation is
    /// pending; repeated submissions emit nothing, and navigation/cancel are busy.
    pub fn dispatch(
        &mut self,
        intent: AuthoringIntent,
    ) -> Result<Vec<AuthoringRequest>, AuthoringError> {
        let result = self.dispatch_inner(intent);
        self.last_error = result.as_ref().err().cloned();
        result
    }

    fn dispatch_inner(
        &mut self,
        intent: AuthoringIntent,
    ) -> Result<Vec<AuthoringRequest>, AuthoringError> {
        self.projection.validate()?;
        if self.pending.is_some() {
            return if matches!(
                intent,
                AuthoringIntent::SubmitCreate
                    | AuthoringIntent::SaveOutfit
                    | AuthoringIntent::ConfirmPlacement
                    | AuthoringIntent::StoreSelected
            ) {
                Ok(vec![])
            } else {
                Err(AuthoringError::Busy)
            };
        }
        match intent {
            AuthoringIntent::SelectProfile(id) => {
                self.projection
                    .profile(&id)
                    .ok_or(AuthoringError::UnknownProfile)?;
                self.selected_profile = Some(id);
                self.selected_home = None;
                self.selected_instance = None;
                self.draft = None;
            }
            AuthoringIntent::OpenCreate => {
                if self.projection.profiles.len() >= MAX_PROFILES {
                    return Err(AuthoringError::ProfileLimit);
                }
                self.draft = Some(AuthoringDraft::Creation(CharacterDraft {
                    name: String::new(),
                    identity: VisualIdentity::Maya,
                    look_id: "maya-everyday".into(),
                }));
            }
            AuthoringIntent::UpdateName(name) => {
                let Some(AuthoringDraft::Creation(draft)) = &mut self.draft else {
                    return Err(AuthoringError::WrongEditor);
                };
                draft.name = name;
            }
            AuthoringIntent::SelectIdentity(identity) => {
                let Some(AuthoringDraft::Creation(draft)) = &mut self.draft else {
                    return Err(AuthoringError::WrongEditor);
                };
                let style = draft
                    .look_id
                    .style_for(draft.identity)
                    .ok_or(AuthoringError::InvalidLook)?;
                draft.identity = identity;
                draft.look_id = identity.look_id(style);
            }
            AuthoringIntent::SelectLook(look_id) => match &mut self.draft {
                Some(AuthoringDraft::Creation(draft)) => {
                    if look_id.style_for(draft.identity).is_none() {
                        return Err(AuthoringError::InvalidLook);
                    }
                    draft.look_id = look_id;
                }
                Some(AuthoringDraft::Outfit(draft)) => {
                    let profile = self
                        .projection
                        .profile(&draft.character_id)
                        .ok_or(AuthoringError::UnknownProfile)?;
                    if look_id.style_for(profile.identity).is_none() {
                        return Err(AuthoringError::InvalidLook);
                    }
                    draft.look_id = look_id;
                }
                _ => return Err(AuthoringError::WrongEditor),
            },
            AuthoringIntent::SubmitCreate => {
                if !matches!(self.draft, Some(AuthoringDraft::Creation(_))) {
                    return Err(AuthoringError::WrongEditor);
                }
                return self.submit(self.draft_request_kind()?);
            }
            AuthoringIntent::OpenOutfit => {
                let profile = self.selected_actor()?;
                self.draft = Some(AuthoringDraft::Outfit(OutfitDraft {
                    character_id: profile.character.id.clone(),
                    look_id: profile.look_id.clone(),
                }));
            }
            AuthoringIntent::SaveOutfit => {
                if !matches!(self.draft, Some(AuthoringDraft::Outfit(_))) {
                    return Err(AuthoringError::WrongEditor);
                }
                return self.submit(self.draft_request_kind()?);
            }
            AuthoringIntent::OpenHome(owner_id) => {
                self.selected_actor()?;
                self.projection
                    .home(&owner_id)
                    .ok_or(AuthoringError::UnknownProfile)?;
                self.selected_home = Some(owner_id);
                self.selected_instance = None;
                self.draft = None;
            }
            AuthoringIntent::SelectCatalog(catalog_id) => {
                let (actor_id, home_owner_id) = self.home_context()?;
                self.projection
                    .catalog_item(&catalog_id)
                    .ok_or(AuthoringError::UnknownCatalogItem)?;
                self.draft = Some(AuthoringDraft::Placement(PlacementDraft {
                    actor_id,
                    home_owner_id,
                    source: PlacementSource::Catalog(catalog_id),
                    pose: default_pose(),
                }));
                self.selected_instance = None;
            }
            AuthoringIntent::SelectOwned(instance_id) => {
                let (_, owner_id) = self.home_context()?;
                let instance = self
                    .projection
                    .home(&owner_id)
                    .and_then(|home| home.instance(&instance_id))
                    .ok_or(AuthoringError::UnknownInstance)?;
                if instance.placement.is_none() {
                    return Err(AuthoringError::WrongPlacementState);
                }
                self.selected_instance = Some(instance_id);
                self.draft = None;
            }
            AuthoringIntent::BeginMove => {
                let (actor_id, home_owner_id) = self.home_context()?;
                let instance_id = self
                    .selected_instance
                    .clone()
                    .ok_or(AuthoringError::NoSelection)?;
                let instance = self
                    .projection
                    .home(&home_owner_id)
                    .and_then(|home| home.instance(&instance_id))
                    .ok_or(AuthoringError::UnknownInstance)?;
                let pose = instance
                    .placement
                    .ok_or(AuthoringError::WrongPlacementState)?;
                self.draft = Some(AuthoringDraft::Placement(PlacementDraft {
                    actor_id,
                    home_owner_id,
                    source: PlacementSource::Move(instance_id),
                    pose,
                }));
            }
            AuthoringIntent::BeginPlace(instance_id) => {
                let (actor_id, home_owner_id) = self.home_context()?;
                let instance = self
                    .projection
                    .home(&home_owner_id)
                    .and_then(|home| home.instance(&instance_id))
                    .ok_or(AuthoringError::UnknownInstance)?;
                if instance.placement.is_some() {
                    return Err(AuthoringError::WrongPlacementState);
                }
                self.draft = Some(AuthoringDraft::Placement(PlacementDraft {
                    actor_id,
                    home_owner_id,
                    source: PlacementSource::Inventory(instance_id),
                    pose: default_pose(),
                }));
                self.selected_instance = None;
            }
            AuthoringIntent::SetCell(cell) => {
                self.placement_draft_mut()?.pose.cell = cell;
            }
            AuthoringIntent::MoveCandidate { dx, dy } => {
                let draft = self.placement_draft_mut()?;
                let x = draft
                    .pose
                    .cell
                    .x
                    .checked_add(dx)
                    .ok_or(AuthoringError::OutOfBounds)?;
                let y = draft
                    .pose
                    .cell
                    .y
                    .checked_add(dy)
                    .ok_or(AuthoringError::OutOfBounds)?;
                draft.pose.cell = GridCell { x, y };
            }
            AuthoringIntent::RotateCandidate => {
                let Some(AuthoringDraft::Placement(draft)) = &self.draft else {
                    return Err(AuthoringError::WrongEditor);
                };
                let catalog_id = match &draft.source {
                    PlacementSource::Catalog(id) => id,
                    PlacementSource::Move(id) | PlacementSource::Inventory(id) => {
                        &self
                            .projection
                            .home(&draft.home_owner_id)
                            .and_then(|home| home.instance(id))
                            .ok_or(AuthoringError::UnknownInstance)?
                            .catalog_id
                    }
                };
                if !self
                    .projection
                    .catalog_item(catalog_id)
                    .ok_or(AuthoringError::UnknownCatalogItem)?
                    .can_rotate()
                {
                    return Err(AuthoringError::Unavailable(
                        "This item does not need rotation".into(),
                    ));
                }
                let draft = self.placement_draft_mut()?;
                draft.pose.direction = draft.pose.direction.clockwise();
            }
            AuthoringIntent::ConfirmPlacement => {
                if !matches!(self.draft, Some(AuthoringDraft::Placement(_))) {
                    return Err(AuthoringError::WrongEditor);
                }
                return self.submit(self.draft_request_kind()?);
            }
            AuthoringIntent::StoreSelected => {
                let (actor_id, home_owner_id) = self.home_context()?;
                let instance_id = self
                    .selected_instance
                    .clone()
                    .ok_or(AuthoringError::NoSelection)?;
                return self.submit(AuthoringRequestKind::StoreInstance {
                    actor_id,
                    home_owner_id,
                    instance_id,
                });
            }
            AuthoringIntent::Cancel => {
                self.draft = None;
            }
            AuthoringIntent::Close => {
                self.draft = None;
                self.selected_home = None;
                self.selected_instance = None;
            }
        }
        Ok(vec![])
    }

    /// Unknown, duplicate, wrong-base, and stale replies are harmless no-ops.
    /// A matching commit validates the complete snapshot and exact requested delta
    /// before changing any projection, selection, draft, or pending operation.
    pub fn receive(&mut self, event: AuthoringEvent) -> Result<(), AuthoringError> {
        if let AuthoringEvent::ProjectionReplaced { projection } = event {
            return self.replace_projection(projection);
        }
        let (operation_id, base_revision) = match &event {
            AuthoringEvent::Committed {
                operation_id,
                base_revision,
                ..
            }
            | AuthoringEvent::Rejected {
                operation_id,
                base_revision,
                ..
            } => (operation_id, *base_revision),
            AuthoringEvent::ProjectionReplaced { .. } => unreachable!(),
        };
        let Some(request) = &self.pending else {
            return Ok(());
        };
        if &request.operation_id != operation_id
            || request.base_revision != base_revision
            || self.projection.revision != base_revision
        {
            return Ok(());
        }
        match event {
            AuthoringEvent::Committed {
                operation_id,
                base_revision,
                projection,
                outcome,
            } => {
                if projection.revision <= self.projection.revision {
                    return Ok(());
                }
                projection.validate()?;
                let expected = after_outcome(
                    &self.projection,
                    &request.kind,
                    &outcome,
                    projection.revision,
                )?;
                if expected != projection {
                    return Err(AuthoringError::InvalidOutcome);
                }
                let commit = AuthoringCommit {
                    operation_id,
                    base_revision,
                    revision: projection.revision,
                    outcome: outcome.clone(),
                };
                match &outcome {
                    AuthoringOutcome::ProfileCreated { character_id } => {
                        self.selected_profile = Some(character_id.clone());
                        self.selected_home = None;
                        self.selected_instance = None;
                    }
                    AuthoringOutcome::Purchased { instance_id, .. }
                    | AuthoringOutcome::Moved { instance_id, .. }
                    | AuthoringOutcome::Placed { instance_id, .. } => {
                        self.selected_instance = Some(instance_id.clone())
                    }
                    AuthoringOutcome::Stored { .. } => self.selected_instance = None,
                    AuthoringOutcome::OutfitSaved { .. } => {}
                }
                self.projection = projection;
                self.draft = None;
                self.pending = None;
                self.last_error = None;
                self.last_commit = Some(commit);
            }
            AuthoringEvent::Rejected { error, .. } => {
                self.pending = None;
                self.last_error = Some(bounded_error(error));
            }
            AuthoringEvent::ProjectionReplaced { .. } => unreachable!(),
        }
        Ok(())
    }

    fn selected_actor(&self) -> Result<&AuthoringProfile, AuthoringError> {
        let id = self
            .selected_profile
            .as_ref()
            .ok_or(AuthoringError::NoSelection)?;
        let profile = self
            .projection
            .profile(id)
            .ok_or(AuthoringError::UnknownProfile)?;
        ensure_available(&profile.character.availability)?;
        Ok(profile)
    }

    fn home_context(&self) -> Result<(CharacterId, CharacterId), AuthoringError> {
        let actor_id = self.selected_actor()?.character.id.clone();
        let owner_id = self
            .selected_home
            .clone()
            .ok_or(AuthoringError::NoSelection)?;
        self.projection
            .home(&owner_id)
            .ok_or(AuthoringError::UnknownProfile)?;
        Ok((actor_id, owner_id))
    }

    fn placement_draft_mut(&mut self) -> Result<&mut PlacementDraft, AuthoringError> {
        match &mut self.draft {
            Some(AuthoringDraft::Placement(draft)) => Ok(draft),
            _ => Err(AuthoringError::WrongEditor),
        }
    }

    fn draft_request_kind(&self) -> Result<AuthoringRequestKind, AuthoringError> {
        match &self.draft {
            Some(AuthoringDraft::Creation(draft)) => Ok(AuthoringRequestKind::CreateProfile {
                name: normalize_profile_name(&draft.name)?,
                identity: draft.identity,
                look_id: draft.look_id.clone(),
            }),
            Some(AuthoringDraft::Outfit(draft)) => Ok(AuthoringRequestKind::SetOutfit {
                actor_id: draft.character_id.clone(),
                look_id: draft.look_id.clone(),
            }),
            Some(AuthoringDraft::Placement(draft)) => {
                let actor_id = draft.actor_id.clone();
                let home_owner_id = draft.home_owner_id.clone();
                let pose = draft.pose;
                Ok(match &draft.source {
                    PlacementSource::Catalog(catalog_id) => AuthoringRequestKind::BuyAndPlace {
                        actor_id,
                        home_owner_id,
                        catalog_id: catalog_id.clone(),
                        pose,
                    },
                    PlacementSource::Move(instance_id) => AuthoringRequestKind::MoveInstance {
                        actor_id,
                        home_owner_id,
                        instance_id: instance_id.clone(),
                        pose,
                    },
                    PlacementSource::Inventory(instance_id) => AuthoringRequestKind::PlaceOwned {
                        actor_id,
                        home_owner_id,
                        instance_id: instance_id.clone(),
                        pose,
                    },
                })
            }
            None => Err(AuthoringError::WrongEditor),
        }
    }

    fn submit(
        &mut self,
        kind: AuthoringRequestKind,
    ) -> Result<Vec<AuthoringRequest>, AuthoringError> {
        validate_kind(&self.projection, &kind)?;
        let next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or(AuthoringError::OperationLimit)?;
        let request = AuthoringRequest {
            operation_id: format!("authoring-{}", self.next_operation).into(),
            base_revision: self.projection.revision,
            kind,
        };
        self.next_operation = next_operation;
        self.pending = Some(request.clone());
        Ok(vec![request])
    }

    fn replace_projection(
        &mut self,
        projection: AuthoringProjection,
    ) -> Result<(), AuthoringError> {
        projection.validate()?;
        if projection.revision <= self.projection.revision {
            return Ok(());
        }
        if self.selected_profile.as_ref().is_some_and(|id| {
            !projection
                .profile(id)
                .is_some_and(|profile| profile.character.availability.is_available())
        }) {
            self.selected_profile = None;
        }
        if self.selected_profile.is_none()
            || self
                .selected_home
                .as_ref()
                .is_some_and(|id| projection.home(id).is_none())
        {
            self.selected_home = None;
        }
        self.projection = projection;
        self.draft = None;
        self.pending = None;
        self.selected_instance = None;
        self.last_error = None;
        self.last_commit = None;
        Ok(())
    }
}

fn default_pose() -> GridPose {
    GridPose {
        cell: GridCell { x: 1, y: 1 },
        direction: Direction::North,
    }
}

fn bounded_error(error: AuthoringError) -> AuthoringError {
    fn reason(value: String) -> String {
        let mut result = String::new();
        for character in value.chars().filter(|character| !character.is_control()) {
            if result.len() + character.len_utf8() > 256 {
                break;
            }
            result.push(character);
        }
        if result.trim().is_empty() {
            "The preview change was rejected".into()
        } else {
            result
        }
    }
    match error {
        AuthoringError::InvalidProjection(value) => {
            AuthoringError::InvalidProjection(reason(value))
        }
        AuthoringError::InvalidName(value) => AuthoringError::InvalidName(reason(value)),
        AuthoringError::Unavailable(value) => AuthoringError::Unavailable(reason(value)),
        AuthoringError::Rejected(value) => AuthoringError::Rejected(reason(value)),
        error => error,
    }
}
