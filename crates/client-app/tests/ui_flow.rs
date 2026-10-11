use wonderland_client_app::{ShellState, preview_projection};
use wonderland_contracts::*;

fn city() -> ShellState {
    let mut state = ShellState::new(preview_projection());
    state
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    state.dispatch(UiIntent::Play).unwrap();
    state
}

fn travel(state: &mut ShellState) -> UiRequest {
    state
        .dispatch(UiIntent::SelectPlace("harbor-cafe".into()))
        .unwrap();
    state.dispatch(UiIntent::Visit).unwrap().remove(0)
}

fn accept(state: &mut ShellState, request: &UiRequest) {
    state
        .receive(UiEvent::Accepted {
            operation_id: request.operation_id.clone(),
            projection_revision: request.projection_revision,
        })
        .unwrap();
}

fn lot() -> ShellState {
    let mut state = city();
    let request = travel(&mut state);
    accept(&mut state, &request);
    state
        .dispatch(UiIntent::SelectObject(EntityRef {
            id: "coffee-machine".into(),
            generation: 1,
        }))
        .unwrap();
    state
}

fn coffee() -> UiIntent {
    UiIntent::TakeOffer {
        target: EntityRef {
            id: "coffee-machine".into(),
            generation: 1,
        },
        action_id: "make-coffee".into(),
        expected_revision: 1,
    }
}

#[test]
fn selection_gates_play_and_five_characters_are_playable() {
    let mut state = ShellState::new(preview_projection());
    assert_eq!(
        state
            .projection
            .characters
            .iter()
            .filter(|c| c.availability.is_available())
            .count(),
        5
    );
    assert!(matches!(
        state.dispatch(UiIntent::Play),
        Err(UiError::NoSelection)
    ));
    assert!(
        state
            .dispatch(UiIntent::SelectCharacter("missing".into()))
            .is_err()
    );
    state
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    state.dispatch(UiIntent::Play).unwrap();
    assert_eq!(state.screen, Screen::City);
    assert_eq!(state.selected_character, Some("maya".into()));
}

#[test]
fn unavailable_travel_never_creates_a_request() {
    let mut state = city();
    state.projection.places[1].availability = Availability::Unavailable {
        reason: "Closed for maintenance".into(),
    };
    let id = state.projection.places[1].id.clone();
    state.dispatch(UiIntent::SelectPlace(id)).unwrap();
    assert!(matches!(
        state.dispatch(UiIntent::Visit),
        Err(UiError::Unavailable(_))
    ));
    assert!(state.pending_requests.is_empty());
    assert_eq!(state.screen, Screen::City);
}

#[test]
fn travel_waits_for_matching_reply_and_rejection_stays_in_city() {
    let mut state = city();
    let request = travel(&mut state);
    assert_eq!(state.screen, Screen::City);
    assert_eq!(state.pending_requests.len(), 1);
    assert!(state.dispatch(UiIntent::Visit).unwrap().is_empty());
    state
        .receive(UiEvent::Accepted {
            operation_id: "unrelated".into(),
            projection_revision: 1,
        })
        .unwrap();
    assert_eq!(state.screen, Screen::City);
    state
        .receive(UiEvent::Accepted {
            operation_id: request.operation_id.clone(),
            projection_revision: 999,
        })
        .unwrap();
    assert_eq!(state.pending_requests.len(), 1);
    state
        .receive(UiEvent::Rejected {
            operation_id: request.operation_id,
            projection_revision: request.projection_revision,
            reason: "Lot full".into(),
        })
        .unwrap();
    assert_eq!(state.screen, Screen::City);
    assert!(state.pending_requests.is_empty());
    assert_eq!(state.last_error, Some(UiError::Rejected("Lot full".into())));
    assert_eq!(state.dispatch(UiIntent::Visit).unwrap().len(), 1);
}

#[test]
fn duplicate_acceptance_is_idempotent_and_back_invalidates_travel() {
    let mut state = city();
    let request = travel(&mut state);
    accept(&mut state, &request);
    assert_eq!(
        state.screen,
        Screen::Lot {
            place_id: "harbor-cafe".into()
        }
    );
    accept(&mut state, &request);
    assert!(state.queue.is_empty());
    state.dispatch(UiIntent::Back).unwrap();
    assert_eq!(state.screen, Screen::City);
    assert_eq!(state.selected_place, Some("harbor-cafe".into()));
    let late = state.dispatch(UiIntent::Visit).unwrap().remove(0);
    state.dispatch(UiIntent::Back).unwrap();
    accept(&mut state, &late);
    assert_eq!(state.screen, Screen::CharacterSelection);
    assert!(state.pending_requests.is_empty());
}

#[test]
fn offers_require_current_generation_revision_and_enabled_action() {
    let mut state = lot();
    for (generation, revision, action) in [
        (2, 1, "make-coffee"),
        (1, 2, "make-coffee"),
        (1, 1, "invented"),
        (1, 1, "clean"),
    ] {
        assert!(
            state
                .dispatch(UiIntent::TakeOffer {
                    target: EntityRef {
                        id: "coffee-machine".into(),
                        generation
                    },
                    action_id: action.into(),
                    expected_revision: revision
                })
                .is_err()
        );
    }
    assert!(state.pending_requests.is_empty());
}

#[test]
fn action_pending_is_not_queue_and_repeated_input_is_suppressed() {
    let mut state = lot();
    let request = state.dispatch(coffee()).unwrap().remove(0);
    assert!(state.queue.is_empty());
    assert_eq!(state.pending_requests.len(), 1);
    assert!(state.dispatch(coffee()).unwrap().is_empty());
    accept(&mut state, &request);
    assert_eq!(state.queue.len(), 1);
    assert_eq!(state.queue[0].action_id, ActionId::from("make-coffee"));
    assert!(state.pending_requests.is_empty());
    accept(&mut state, &request);
    assert_eq!(state.queue.len(), 1);
    assert!(state.dispatch(coffee()).unwrap().is_empty());
}

#[test]
fn cancellation_waits_for_its_own_acknowledgment() {
    let mut state = lot();
    let action = state.dispatch(coffee()).unwrap().remove(0);
    accept(&mut state, &action);
    let cancel = state
        .dispatch(UiIntent::Cancel {
            operation_id: action.operation_id.clone(),
        })
        .unwrap()
        .remove(0);
    assert_eq!(state.queue[0].status, QueueStatus::CancellationPending);
    assert!(
        state
            .dispatch(UiIntent::Cancel {
                operation_id: action.operation_id.clone()
            })
            .unwrap()
            .is_empty()
    );
    state
        .receive(UiEvent::CancellationAcknowledged {
            operation_id: action.operation_id.clone(),
            projection_revision: action.projection_revision,
        })
        .unwrap();
    accept(&mut state, &cancel); // Generic acceptance is not cancellation acknowledgment.
    assert_eq!(state.queue.len(), 1);
    state
        .receive(UiEvent::CancellationAcknowledged {
            operation_id: cancel.operation_id.clone(),
            projection_revision: cancel.projection_revision,
        })
        .unwrap();
    assert!(state.queue.is_empty());
    assert!(state.pending_requests.is_empty());
    state
        .receive(UiEvent::CancellationAcknowledged {
            operation_id: cancel.operation_id,
            projection_revision: cancel.projection_revision,
        })
        .unwrap();
    assert!(state.queue.is_empty());
}

#[test]
fn rejected_cancellation_restores_acknowledged_queue_item() {
    let mut state = lot();
    let action = state.dispatch(coffee()).unwrap().remove(0);
    accept(&mut state, &action);
    let cancel = state
        .dispatch(UiIntent::Cancel {
            operation_id: action.operation_id,
        })
        .unwrap()
        .remove(0);
    state
        .receive(UiEvent::Rejected {
            operation_id: cancel.operation_id,
            projection_revision: cancel.projection_revision,
            reason: "Already running".into(),
        })
        .unwrap();
    assert_eq!(state.queue[0].status, QueueStatus::Active);
    assert!(state.pending_requests.is_empty());
}

#[test]
fn replacement_projection_invalidates_changed_target_and_late_action() {
    let mut state = lot();
    let action = state.dispatch(coffee()).unwrap().remove(0);
    let mut projection = state.projection.clone();
    projection.revision += 1;
    projection.objects[0].target.generation += 1;
    state
        .receive(UiEvent::ProjectionUpdated { projection })
        .unwrap();
    assert!(state.selected_object.is_none());
    assert!(state.pending_requests.is_empty());
    accept(&mut state, &action);
    assert!(state.queue.is_empty());
}

#[test]
fn back_from_lot_preserves_character_and_destination_but_invalidates_action() {
    let mut state = lot();
    let action = state.dispatch(coffee()).unwrap().remove(0);
    state.dispatch(UiIntent::Back).unwrap();
    accept(&mut state, &action);
    assert_eq!(state.screen, Screen::City);
    assert_eq!(state.selected_character, Some("maya".into()));
    assert_eq!(state.selected_place, Some("harbor-cafe".into()));
    assert!(state.selected_object.is_none());
    assert!(state.queue.is_empty());
}

#[test]
fn empty_and_invalid_projections_disable_play_without_panicking() {
    let mut projection = preview_projection();
    projection.characters.clear();
    let mut state = ShellState::new(projection);
    assert!(state.dispatch(UiIntent::Play).is_err());
    let mut projection = preview_projection();
    projection.version = 99;
    let mut state = ShellState::new(projection);
    assert!(matches!(
        state.dispatch(UiIntent::Play),
        Err(UiError::InvalidProjection(_))
    ));
}

#[test]
fn changed_revision_or_removed_character_invalidates_pending_work() {
    let mut state = lot();
    let action = state.dispatch(coffee()).unwrap().remove(0);
    let mut projection = state.projection.clone();
    projection.revision += 1;
    projection.objects[0].revision += 1;
    state
        .receive(UiEvent::ProjectionUpdated { projection })
        .unwrap();
    assert!(state.selected_object.is_none());
    assert!(state.pending_requests.is_empty());
    accept(&mut state, &action);
    assert!(state.queue.is_empty());
    let mut projection = state.projection.clone();
    projection.revision += 1;
    projection
        .characters
        .retain(|c| c.id != CharacterId::from("maya"));
    state
        .receive(UiEvent::ProjectionUpdated { projection })
        .unwrap();
    assert_eq!(state.screen, Screen::CharacterSelection);
    assert!(state.selected_character.is_none());
}

#[test]
fn invalid_or_older_projection_cannot_replace_current_selection() {
    let mut state = lot();
    let current = state.projection.clone();
    let mut invalid = current.clone();
    invalid.revision += 1;
    invalid.objects[0].anchor.x = f32::NAN;
    assert!(
        state
            .receive(UiEvent::ProjectionUpdated {
                projection: invalid
            })
            .is_err()
    );
    assert_eq!(state.projection, current);
    let mut older = current;
    older.characters.clear();
    state
        .receive(UiEvent::ProjectionUpdated { projection: older })
        .unwrap();
    assert_eq!(state.selected_character, Some("maya".into()));
}

#[test]
fn queue_limit_does_not_block_cancelling_the_full_queue() {
    let mut projection = preview_projection();
    projection.objects[0].offers = (0..16)
        .map(|n| ActionOffer {
            id: format!("action-{n}").into(),
            label: "Inspect".into(),
            availability: Availability::Available,
        })
        .collect();
    let mut second = projection.objects[0].clone();
    second.target.id = "second-machine".into();
    projection.objects.push(second);
    let mut state = ShellState::new(projection);
    state
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    state.dispatch(UiIntent::Play).unwrap();
    let request = travel(&mut state);
    accept(&mut state, &request);
    for id in ["coffee-machine", "second-machine"] {
        let target = EntityRef {
            id: id.into(),
            generation: 1,
        };
        state
            .dispatch(UiIntent::SelectObject(target.clone()))
            .unwrap();
        for n in 0..16 {
            let request = state
                .dispatch(UiIntent::TakeOffer {
                    target: target.clone(),
                    action_id: format!("action-{n}").into(),
                    expected_revision: 1,
                })
                .unwrap()
                .remove(0);
            accept(&mut state, &request);
        }
    }
    assert_eq!(state.queue.len(), 32);
    let operation_id = state.queue[0].operation_id.clone();
    assert_eq!(
        state
            .dispatch(UiIntent::Cancel { operation_id })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn pending_interactions_reserve_bounded_queue_capacity() {
    let mut state = lot();
    // Reach the public bound through distinct real offered actions and acknowledgments.
    for n in 0..31 {
        let mut projection = state.projection.clone();
        projection.revision += 1;
        projection.objects[0].offers = vec![ActionOffer {
            id: format!("action-{n}").into(),
            label: "Inspect".into(),
            availability: Availability::Available,
        }];
        state
            .receive(UiEvent::ProjectionUpdated { projection })
            .unwrap();
        let request = state
            .dispatch(UiIntent::TakeOffer {
                target: EntityRef {
                    id: "coffee-machine".into(),
                    generation: 1,
                },
                action_id: format!("action-{n}").into(),
                expected_revision: 1,
            })
            .unwrap()
            .remove(0);
        accept(&mut state, &request);
    }
    let mut projection = state.projection.clone();
    projection.revision += 1;
    projection.objects[0].offers = vec![
        ActionOffer {
            id: "next".into(),
            label: "Inspect".into(),
            availability: Availability::Available,
        },
        ActionOffer {
            id: "overflow".into(),
            label: "Inspect".into(),
            availability: Availability::Available,
        },
    ];
    state
        .receive(UiEvent::ProjectionUpdated { projection })
        .unwrap();
    for (id, allowed) in [("next", true), ("overflow", false)] {
        let result = state.dispatch(UiIntent::TakeOffer {
            target: EntityRef {
                id: "coffee-machine".into(),
                generation: 1,
            },
            action_id: id.into(),
            expected_revision: 1,
        });
        if allowed {
            assert_eq!(result.unwrap().len(), 1);
        } else {
            assert_eq!(result, Err(UiError::OperationLimit));
        }
    }
}
