//! Pure UI reducer. Send emitted requests to an explicit adapter, then return its replies.
//! Public fields are readable presentation state; mutation belongs to `dispatch`/`receive`.
use std::collections::BTreeMap;
use wonderland_contracts::*;

pub mod authoring;

/// Request IDs are unique for this shell lifetime and never reused after navigation.
#[derive(Clone, Debug)]
pub struct ShellState {
    pub screen: Screen,
    pub selected_character: Option<CharacterId>,
    pub selected_place: Option<PlaceId>,
    pub selected_object: Option<EntityRef>,
    pub pending_requests: BTreeMap<OperationId, PendingRequest>,
    pub queue: Vec<QueuedAction>,
    pub projection: UiProjection,
    pub last_error: Option<UiError>,
    next_operation: u64,
}

impl ShellState {
    /// Invalid projections remain readable, with an error and no enabled transitions.
    pub fn new(projection: UiProjection) -> Self {
        let last_error = projection.validate().err();
        Self {
            screen: Screen::CharacterSelection,
            selected_character: None,
            selected_place: None,
            selected_object: None,
            pending_requests: BTreeMap::new(),
            queue: vec![],
            projection,
            last_error,
            next_operation: 1,
        }
    }

    /// Validate intent eligibility, change local selection, or emit one pending request.
    /// Repeated in-flight or queued actions return an empty request vector.
    pub fn dispatch(&mut self, intent: UiIntent) -> Result<Vec<UiRequest>, UiError> {
        let result = self.dispatch_inner(intent);
        self.last_error = result.as_ref().err().cloned();
        result
    }

    fn dispatch_inner(&mut self, intent: UiIntent) -> Result<Vec<UiRequest>, UiError> {
        self.projection.validate()?;
        match intent {
            UiIntent::SelectCharacter(id) => {
                self.require_screen(&Screen::CharacterSelection)?;
                self.projection
                    .characters
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or(UiError::UnknownSelection)?;
                self.selected_character = Some(id);
            }
            UiIntent::Play => {
                self.require_screen(&Screen::CharacterSelection)?;
                let id = self
                    .selected_character
                    .as_ref()
                    .ok_or(UiError::NoSelection)?;
                self.projection
                    .characters
                    .iter()
                    .find(|c| &c.id == id)
                    .ok_or(UiError::UnknownSelection)?
                    .availability
                    .ensure_available()?;
                self.screen = Screen::City;
            }
            UiIntent::SelectPlace(id) => {
                self.require_screen(&Screen::City)?;
                self.projection
                    .places
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or(UiError::UnknownSelection)?;
                if self.selected_place.as_ref() != Some(&id) {
                    self.pending_requests.clear();
                }
                self.selected_place = Some(id);
            }
            UiIntent::Visit => {
                self.require_screen(&Screen::City)?;
                let id = self.selected_place.clone().ok_or(UiError::NoSelection)?;
                self.projection
                    .places
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or(UiError::UnknownSelection)?
                    .availability
                    .ensure_available()?;
                if !self.pending_requests.is_empty() {
                    return Ok(vec![]);
                }
                let character_id = self
                    .selected_character
                    .clone()
                    .ok_or(UiError::NoSelection)?;
                return self.submit(RequestKind::Travel {
                    character_id,
                    place_id: id,
                });
            }
            UiIntent::SelectObject(target) => {
                let Screen::Lot { place_id } = &self.screen else {
                    return Err(UiError::WrongScreen);
                };
                self.projection
                    .objects
                    .iter()
                    .find(|o| o.target == target && &o.place_id == place_id)
                    .ok_or(UiError::StaleTarget)?;
                if self.selected_object.as_ref() != Some(&target) {
                    self.invalidate_pending_interactions();
                }
                self.selected_object = Some(target);
            }
            UiIntent::DismissObject => {
                if !matches!(self.screen, Screen::Lot { .. }) {
                    return Err(UiError::WrongScreen);
                }
                self.selected_object = None;
                self.invalidate_pending_interactions();
            }
            UiIntent::TakeOffer {
                target,
                action_id,
                expected_revision,
            } => {
                let Screen::Lot { place_id } = &self.screen else {
                    return Err(UiError::WrongScreen);
                };
                if self.selected_object.as_ref() != Some(&target) {
                    return Err(UiError::StaleTarget);
                }
                let object = self
                    .projection
                    .objects
                    .iter()
                    .find(|o| {
                        o.target == target
                            && o.revision == expected_revision
                            && &o.place_id == place_id
                    })
                    .ok_or(UiError::StaleTarget)?;
                object
                    .offers
                    .iter()
                    .find(|a| a.id == action_id)
                    .ok_or(UiError::UnofferedAction)?
                    .availability
                    .ensure_available()?;
                if self.queue.iter().any(|q| q.target == target && q.action_id == action_id) || self.pending_requests.values().any(|p| matches!(&p.request.kind, RequestKind::Interaction { target: t, action_id: a, .. } if t == &target && a == &action_id)) { return Ok(vec![]); }
                let character_id = self
                    .selected_character
                    .clone()
                    .ok_or(UiError::NoSelection)?;
                return self.submit(RequestKind::Interaction {
                    character_id,
                    place_id: place_id.clone(),
                    target,
                    action_id,
                    expected_revision,
                });
            }
            UiIntent::Cancel { operation_id } => {
                if !matches!(self.screen, Screen::Lot { .. }) {
                    return Err(UiError::WrongScreen);
                }
                let index = self
                    .queue
                    .iter()
                    .position(|q| q.operation_id == operation_id)
                    .ok_or(UiError::UnknownSelection)?;
                if self.queue[index].status == QueueStatus::CancellationPending {
                    return Ok(vec![]);
                }
                let result = self.submit(RequestKind::Cancellation {
                    queue_operation_id: operation_id,
                })?;
                self.queue[index].status = QueueStatus::CancellationPending;
                return Ok(result);
            }
            UiIntent::Back => {
                self.pending_requests.clear();
                self.queue.clear();
                self.selected_object = None;
                match self.screen {
                    Screen::Lot { .. } => self.screen = Screen::City,
                    Screen::City => {
                        self.screen = Screen::CharacterSelection;
                        self.selected_place = None;
                    }
                    Screen::CharacterSelection => {}
                }
            }
        }
        Ok(vec![])
    }

    /// Apply only matching current-operation replies. Duplicate/unrelated replies are no-ops.
    pub fn receive(&mut self, event: UiEvent) -> Result<(), UiError> {
        if let UiEvent::ProjectionUpdated { projection } = event {
            return self.update_projection(projection);
        }
        let (operation_id, revision) = match &event {
            UiEvent::Accepted {
                operation_id,
                projection_revision,
            }
            | UiEvent::Rejected {
                operation_id,
                projection_revision,
                ..
            }
            | UiEvent::CancellationAcknowledged {
                operation_id,
                projection_revision,
            } => (operation_id, *projection_revision),
            UiEvent::ProjectionUpdated { .. } => unreachable!(),
        };
        let Some(pending) = self.pending_requests.get(operation_id) else {
            return Ok(());
        };
        if pending.request.projection_revision != revision || self.projection.revision != revision {
            return Ok(());
        }
        let request = pending.request.clone();
        match event {
            UiEvent::Accepted { .. } => match request.kind {
                RequestKind::Travel { place_id, .. } => {
                    if self.screen != Screen::City
                        || self.selected_place.as_ref() != Some(&place_id)
                    {
                        return Ok(());
                    }
                    self.screen = Screen::Lot { place_id };
                }
                RequestKind::Interaction {
                    target,
                    action_id,
                    expected_revision,
                    place_id,
                    ..
                } => {
                    if self.screen != (Screen::Lot { place_id }) {
                        return Ok(());
                    }
                    let Some(object) = self
                        .projection
                        .objects
                        .iter()
                        .find(|o| o.target == target && o.revision == expected_revision)
                    else {
                        return Ok(());
                    };
                    let Some(offer) = object
                        .offers
                        .iter()
                        .find(|a| a.id == action_id && a.availability.is_available())
                    else {
                        return Ok(());
                    };
                    self.queue.push(QueuedAction {
                        operation_id: request.operation_id.clone(),
                        target,
                        action_id,
                        expected_revision,
                        label: offer.label.clone(),
                        status: QueueStatus::Active,
                    });
                }
                RequestKind::Cancellation { .. } => return Ok(()),
            },
            UiEvent::CancellationAcknowledged { .. } => {
                let RequestKind::Cancellation { queue_operation_id } = request.kind else {
                    return Ok(());
                };
                self.queue.retain(|q| q.operation_id != queue_operation_id);
            }
            UiEvent::Rejected { reason, .. } => {
                if let RequestKind::Cancellation { queue_operation_id } = request.kind
                    && let Some(q) = self
                        .queue
                        .iter_mut()
                        .find(|q| q.operation_id == queue_operation_id)
                {
                    q.status = QueueStatus::Active;
                }
                self.last_error = Some(UiError::Rejected(reason.chars().take(256).collect()));
            }
            UiEvent::ProjectionUpdated { .. } => unreachable!(),
        }
        self.pending_requests.remove(&request.operation_id);
        Ok(())
    }

    fn require_screen(&self, screen: &Screen) -> Result<(), UiError> {
        if &self.screen == screen {
            Ok(())
        } else {
            Err(UiError::WrongScreen)
        }
    }
    fn submit(&mut self, kind: RequestKind) -> Result<Vec<UiRequest>, UiError> {
        let reserved_queue_slots = self.queue.len()
            + self
                .pending_requests
                .values()
                .filter(|p| matches!(p.request.kind, RequestKind::Interaction { .. }))
                .count();
        if self.pending_requests.len() >= 32
            || (matches!(kind, RequestKind::Interaction { .. }) && reserved_queue_slots >= 32)
        {
            return Err(UiError::OperationLimit);
        }
        let next = self
            .next_operation
            .checked_add(1)
            .ok_or(UiError::OperationLimit)?;
        let request = UiRequest {
            operation_id: format!("ui-{}", self.next_operation).into(),
            projection_revision: self.projection.revision,
            kind,
        };
        self.next_operation = next;
        self.pending_requests.insert(
            request.operation_id.clone(),
            PendingRequest {
                request: request.clone(),
                status: RequestStatus::Pending,
            },
        );
        Ok(vec![request])
    }
    fn invalidate_pending_interactions(&mut self) {
        self.pending_requests
            .retain(|_, p| !matches!(p.request.kind, RequestKind::Interaction { .. }));
    }
    fn update_projection(&mut self, projection: UiProjection) -> Result<(), UiError> {
        projection.validate()?;
        if projection.revision <= self.projection.revision {
            return Ok(());
        }
        self.pending_requests.clear();
        self.queue.retain(|q| {
            projection
                .objects
                .iter()
                .any(|o| o.target == q.target && o.revision == q.expected_revision)
        });
        for q in &mut self.queue {
            q.status = QueueStatus::Active;
        }
        if self.selected_character.as_ref().is_some_and(|id| {
            !projection
                .characters
                .iter()
                .any(|c| &c.id == id && c.availability.is_available())
        }) {
            self.selected_character = None;
            self.selected_place = None;
            self.selected_object = None;
            self.screen = Screen::CharacterSelection;
            self.queue.clear();
        }
        if self.selected_place.as_ref().is_some_and(|id| {
            !projection
                .places
                .iter()
                .any(|p| &p.id == id && p.availability.is_available())
        }) {
            self.selected_place = None;
            self.selected_object = None;
            if matches!(self.screen, Screen::Lot { .. }) {
                self.screen = Screen::City;
            }
            self.queue.clear();
        }
        if self.selected_object.as_ref().is_some_and(|target| {
            !projection.objects.iter().any(|o| {
                &o.target == target
                    && self
                        .projection
                        .objects
                        .iter()
                        .any(|old| old.target == o.target && old.revision == o.revision)
            })
        }) {
            self.selected_object = None;
        }
        self.projection = projection;
        self.last_error = None;
        Ok(())
    }
}

/// Repository-owned deterministic fixture. No transport, timers, DOM, or renderer.
pub fn preview_projection() -> UiProjection {
    serde_json::from_str(include_str!("../../../fixtures/ui/preview-v1.json"))
        .expect("checked-in preview fixture must deserialize")
}
