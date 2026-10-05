use wonderland_client_app::authoring::*;
use wonderland_client_app::{ShellState, preview_projection};
use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, UiEvent, UiIntent};

fn with_wardrobe(mut projection: AuthoringProjection) -> AuthoringProjection {
    for profile in &mut projection.profiles {
        profile.appearance.head = Some("test:head-one".into());
        profile.appearance.body = Some("test:body-one".into());
        profile.wardrobe.categories = vec![WardrobeCategory {
            id: "day".into(), label: "Day".into(), slot: AppearanceSlot::Body,
        }];
        profile.wardrobe.outfits = [("default", "test:body-one"), ("alternative", "test:body-two")].into_iter().map(|(label, content_key)| OwnedOutfit {
            id: format!("test-{}-{label}", profile.character.id).into(),
            content_key: content_key.into(), category_id: "day".into(), label: label.into(),
            thumbnail: None, is_default: label == "default",
            actions: vec![WardrobeActionOffer { action: WardrobeAction::Change, availability: Availability::Available }],
        }).collect();
    }
    projection
}

fn state_and_provider() -> (AuthoringState, PreviewAuthoringProvider) {
    let projection = with_wardrobe(preview_authoring_projection());
    (
        AuthoringState::new(projection.clone()),
        PreviewAuthoringProvider::new(projection).unwrap(),
    )
}

fn creation(state: &mut AuthoringState, name: &str) -> AuthoringRequest {
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName(name.into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0)
}

fn own_home(state: &mut AuthoringState) {
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
}

fn buy(state: &mut AuthoringState, catalog_id: &str) -> AuthoringRequest {
    state
        .dispatch(AuthoringIntent::SelectCatalog(catalog_id.into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SetCell(GridCell { x: 2, y: 2 }))
        .unwrap();
    state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0)
}

#[test]
fn creation_is_a_draft_until_a_matching_commit_returns_its_new_identity() {
    let (mut state, mut provider) = state_and_provider();
    let before = state.projection().clone();
    let request = creation(&mut state, "  Zoë 李  ");
    assert_eq!(state.projection(), &before);
    assert_eq!(provider.snapshot(), &before);
    assert!(state.selected_profile().is_none());
    assert_eq!(state.pending(), Some(&request));
    assert!(
        matches!(&request.kind, AuthoringRequestKind::CreateProfile { name, .. } if name == "Zoë 李")
    );
    assert!(
        state
            .dispatch(AuthoringIntent::SubmitCreate)
            .unwrap()
            .is_empty()
    );
    for intent in [
        AuthoringIntent::Cancel,
        AuthoringIntent::Close,
        AuthoringIntent::SelectProfile("maya".into()),
    ] {
        assert_eq!(state.dispatch(intent), Err(AuthoringError::Busy));
    }
    let event = provider.handle(&request);
    assert_eq!(
        state.projection(),
        &before,
        "provider replies are not applied implicitly"
    );
    let AuthoringEvent::Committed {
        outcome: AuthoringOutcome::ProfileCreated { character_id },
        ..
    } = &event
    else {
        panic!("expected typed created identity")
    };
    let created_id = character_id.clone();
    state.receive(event.clone()).unwrap();
    assert_eq!(state.projection().profiles.len(), 6);
    assert_eq!(
        state
            .projection()
            .profile(&created_id)
            .unwrap()
            .character
            .name,
        "Zoë 李"
    );
    assert_eq!(state.selected_profile(), Some(&created_id));
    assert!(state.pending().is_none());
    assert!(state.draft().is_none());
    assert_eq!(
        state.last_commit().unwrap().operation_id,
        request.operation_id
    );
    state.receive(event).unwrap();
    assert_eq!(state.projection().profiles.len(), 6);
}

#[test]
fn creation_preserves_independent_head_body_skin_gender_description_and_shard() {
    let mut projection = preview_authoring_projection();
    let option = |key: &str| AppearanceOption {
        key: key.into(), label: key.into(), thumbnail: None, availability: Availability::Available,
        genders: vec!["female".into()], skin_tones: vec!["medium".into()],
    };
    projection.appearance_content.heads = vec![option("source-head-one"), option("source-head-two")];
    projection.appearance_content.bodies = vec![option("source-body-one"), option("source-body-two")];
    projection.appearance_content.requirements.head = true;
    projection.appearance_content.requirements.body = true;
    projection.account.shards = vec![ShardOption { id: "city-7".into(), label: "City Seven".into(), availability: Availability::Available }];
    projection.account.default_shard = Some("city-7".into());
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection);
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state.dispatch(AuthoringIntent::UpdateName("New neighbor".into())).unwrap();
    state.dispatch(AuthoringIntent::UpdateDescription("A carefully chosen appearance.".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectGender("female".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectSkinTone("medium".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectHead("source-head-one".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectBody("source-body-two".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectHead("source-head-two".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectGender("male".into())).unwrap();
    assert!(matches!(state.draft_validity(), Err(AuthoringError::InvalidAppearance(_))));
    assert!(matches!(state.draft(), Some(AuthoringDraft::Creation(draft)) if draft.appearance.body.as_ref().unwrap().as_ref() == "source-body-two"));
    state.dispatch(AuthoringIntent::SelectGender("female".into())).unwrap();
    let request = state.dispatch(AuthoringIntent::SubmitCreate).unwrap().remove(0);
    assert_eq!(request.expected_sources.appearance, 1);
    state.receive(provider.handle(&request)).unwrap();
    let created = state.projection().profile(state.selected_profile().unwrap()).unwrap();
    assert_eq!(created.appearance.head.as_ref().unwrap().as_ref(), "source-head-two");
    assert_eq!(created.appearance.body.as_ref().unwrap().as_ref(), "source-body-two");
    assert_eq!(created.appearance.skin_tone.as_ref().unwrap().as_ref(), "medium");
    assert_eq!(created.appearance.gender.as_ref().unwrap().as_ref(), "female");
    assert_eq!(created.description, "A carefully chosen appearance.");
    assert_eq!(created.shard_id.as_ref().unwrap().as_ref(), "city-7");
    assert!(created.portrait.is_none());
}

#[test]
fn raw_name_input_is_preserved_in_the_draft_and_invalid_names_cannot_submit() {
    let (mut state, _) = state_and_provider();
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    for input in ["".into(), "  ".into(), "a\nb".into(), "x".repeat(33)] {
        state
            .dispatch(AuthoringIntent::UpdateName(input.clone()))
            .unwrap();
        assert!(
            matches!(state.draft(), Some(AuthoringDraft::Creation(draft)) if draft.name == input)
        );
        assert!(matches!(
            state.draft_validity(),
            Err(AuthoringError::InvalidName(_))
        ));
        assert!(state.dispatch(AuthoringIntent::SubmitCreate).is_err());
        assert!(state.pending().is_none());
        assert_eq!(state.projection().profiles.len(), 5);
    }
    state
        .dispatch(AuthoringIntent::UpdateName("  e\u{301} 李  ".into()))
        .unwrap();
    assert!(
        matches!(state.draft(), Some(AuthoringDraft::Creation(draft)) if draft.name == "  e\u{301} 李  ")
    );
    state.draft_validity().unwrap();
}

#[test]
fn supplied_capacity_is_enforced_without_invalidating_saved_overflow() {
    let mut projection = preview_authoring_projection();
    projection.account.profile_capacity = CapacityPolicy::Limited { maximum: 8 };
    let mut state = AuthoringState::new(projection.clone());
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    for name in ["Six", "Seven", "Eight"] {
        let request = creation(&mut state, name);
        state.receive(provider.handle(&request)).unwrap();
    }
    let before = state.projection().clone();
    assert_eq!(before.profiles.len(), 8);
    assert_eq!(
        state.dispatch(AuthoringIntent::OpenCreate),
        Err(AuthoringError::ProfileLimit)
    );
    let kind = AuthoringRequestKind::CreateProfile {
        name: "Nine".into(), description: String::new(), shard_id: None,
        appearance: before.appearance_content.default_selection(),
    };
    let request = AuthoringRequest {
        operation_id: "external-ninth".into(),
        base_revision: before.revision,
        expected_sources: before.source_revisions(&kind),
        kind,
    };
    assert!(matches!(
        provider.handle(&request),
        AuthoringEvent::Rejected {
            error: AuthoringError::ProfileLimit,
            ..
        }
    ));
    assert_eq!(provider.snapshot(), &before);
    assert_eq!(state.projection(), &before);
}

#[test]
fn outfit_cancel_preserves_appearance_and_save_changes_only_the_selected_profile() {
    let (mut state, mut provider) = state_and_provider();
    state
        .dispatch(AuthoringIntent::SelectProfile("jules".into()))
        .unwrap();
    let before = state.projection().clone();
    state.dispatch(AuthoringIntent::OpenOutfit).unwrap();
    state
        .dispatch(AuthoringIntent::SelectOwnedOutfit("test-jules-alternative".into()))
        .unwrap();
    state.dispatch(AuthoringIntent::Cancel).unwrap();
    assert_eq!(state.projection(), &before);
    state.dispatch(AuthoringIntent::OpenOutfit).unwrap();
    assert_eq!(
        state.dispatch(AuthoringIntent::SelectOwnedOutfit("test-maya-alternative".into())),
        Err(AuthoringError::UnknownOutfit)
    );
    state
        .dispatch(AuthoringIntent::SelectOwnedOutfit("test-jules-alternative".into()))
        .unwrap();
    assert_eq!(
        state.dispatch(AuthoringIntent::UpdateName("Someone else".into())),
        Err(AuthoringError::WrongEditor)
    );
    let request = state
        .dispatch(AuthoringIntent::SaveOutfit)
        .unwrap()
        .remove(0);
    assert_eq!(state.projection(), &before);
    state.receive(provider.handle(&request)).unwrap();
    let mut expected = before;
    expected.revision += 1;
    expected.profiles[1].appearance.body = Some("test:body-two".into());
    expected.profiles[1].wardrobe.revision += 1;
    assert_eq!(state.projection(), &expected);
    assert!(state.draft().is_none());
}

#[test]
fn wrong_operation_base_or_nonnewer_receipts_leave_the_current_draft_and_pending_request() {
    let (mut state, mut provider) = state_and_provider();
    let request = creation(&mut state, "Six");
    let event = provider.handle(&request);
    let before = state.projection().clone();
    let draft = state.draft().cloned();
    let mut wrong_operation = event.clone();
    if let AuthoringEvent::Committed { operation_id, .. } = &mut wrong_operation {
        *operation_id = "unrelated".into();
    }
    let mut wrong_base = event.clone();
    if let AuthoringEvent::Committed { base_revision, .. } = &mut wrong_base {
        *base_revision += 1;
    }
    let mut old_snapshot = event.clone();
    if let AuthoringEvent::Committed { projection, .. } = &mut old_snapshot {
        projection.revision = before.revision;
    }
    for bad in [wrong_operation, wrong_base, old_snapshot] {
        state.receive(bad).unwrap();
        assert_eq!(state.projection(), &before);
        assert_eq!(state.draft(), draft.as_ref());
        assert_eq!(state.pending(), Some(&request));
    }
    state.receive(event).unwrap();
    assert_eq!(state.projection().profiles.len(), 6);
}

#[test]
fn invalid_snapshots_and_mismatched_outcomes_are_rejected_atomically() {
    let (mut state, mut provider) = state_and_provider();
    let request = creation(&mut state, "Six");
    let valid_event = provider.handle(&request);
    let before = state.projection().clone();
    let draft = state.draft().cloned();
    let mut invalid_snapshot = valid_event.clone();
    if let AuthoringEvent::Committed { projection, .. } = &mut invalid_snapshot {
        projection.profiles[0].character.money = -1;
    }
    assert!(matches!(
        state.receive(invalid_snapshot),
        Err(AuthoringError::InvalidProjection(_))
    ));
    let mut wrong_identity = valid_event.clone();
    if let AuthoringEvent::Committed { outcome, .. } = &mut wrong_identity {
        *outcome = AuthoringOutcome::ProfileCreated {
            character_id: "maya".into(),
        };
    }
    assert_eq!(
        state.receive(wrong_identity),
        Err(AuthoringError::InvalidOutcome)
    );
    let mut unrelated_change = valid_event.clone();
    if let AuthoringEvent::Committed { projection, .. } = &mut unrelated_change {
        projection.profiles[0].character.money = 1_249;
    }
    assert_eq!(
        state.receive(unrelated_change),
        Err(AuthoringError::InvalidOutcome)
    );
    assert_eq!(state.projection(), &before);
    assert_eq!(state.pending(), Some(&request));
    assert_eq!(state.draft(), draft.as_ref());
    state.receive(valid_event).unwrap();
}

#[test]
fn an_old_duplicate_commit_cannot_close_a_new_editor_or_pending_operation() {
    let (mut state, mut provider) = state_and_provider();
    let created = creation(&mut state, "Six");
    let old_event = provider.handle(&created);
    state.receive(old_event.clone()).unwrap();
    let outfit = creation(&mut state, "Seven");
    let draft = state.draft().cloned();
    state.receive(old_event).unwrap();
    assert_eq!(state.pending(), Some(&outfit));
    assert_eq!(state.draft(), draft.as_ref());
    state.receive(provider.handle(&outfit)).unwrap();
    assert_eq!(state.projection().profiles.len(), 7);
}

#[test]
fn explicit_replacement_invalidates_pending_work_without_reusing_operation_ids() {
    let (mut state, mut provider) = state_and_provider();
    let old_request = creation(&mut state, "Old draft");
    let old_event = provider.handle(&old_request);
    let mut replacement = preview_authoring_projection();
    replacement.revision = 10;
    state
        .receive(AuthoringEvent::ProjectionReplaced {
            projection: replacement.clone(),
        })
        .unwrap();
    assert_eq!(state.projection(), &replacement);
    assert!(state.pending().is_none());
    assert!(state.draft().is_none());
    assert!(state.last_commit().is_none());
    let new_request = creation(&mut state, "New draft");
    assert_ne!(old_request.operation_id, new_request.operation_id);
    assert_eq!(new_request.base_revision, 10);
    state.receive(old_event).unwrap();
    assert_eq!(state.pending(), Some(&new_request));
    assert_eq!(state.projection(), &replacement);
}

#[test]
fn invalid_or_old_replacement_does_not_cancel_current_work() {
    let (mut state, _) = state_and_provider();
    let request = creation(&mut state, "Six");
    let original = state.projection().clone();
    let mut invalid = original.clone();
    invalid.version = 77;
    invalid.revision += 1;
    assert!(
        state
            .receive(AuthoringEvent::ProjectionReplaced {
                projection: invalid
            })
            .is_err()
    );
    state
        .receive(AuthoringEvent::ProjectionReplaced {
            projection: original.clone(),
        })
        .unwrap();
    assert_eq!(state.pending(), Some(&request));
    assert_eq!(state.projection(), &original);
    let mut invalid = original;
    invalid.version = AUTHORING_VERSION + 1;
    let mut invalid_state = AuthoringState::new(invalid);
    assert!(matches!(
        invalid_state.last_error(),
        Some(AuthoringError::InvalidProjection(_))
    ));
    assert!(invalid_state.dispatch(AuthoringIntent::OpenCreate).is_err());
}

#[test]
fn matching_rejection_retains_the_candidate_and_retry_uses_a_fresh_operation() {
    let (mut state, mut provider) = state_and_provider();
    own_home(&mut state);
    let first = buy(&mut state, "armchair");
    let draft = state.draft().cloned();
    let before = state.projection().clone();
    state
        .receive(AuthoringEvent::Rejected {
            operation_id: "wrong".into(),
            base_revision: first.base_revision,
            error: AuthoringError::Rejected("Try again".into()),
        })
        .unwrap();
    assert_eq!(state.pending(), Some(&first));
    state
        .receive(AuthoringEvent::Rejected {
            operation_id: first.operation_id.clone(),
            base_revision: first.base_revision,
            error: AuthoringError::Rejected("Try again".into()),
        })
        .unwrap();
    assert!(state.pending().is_none());
    assert_eq!(state.draft(), draft.as_ref());
    assert_eq!(state.projection(), &before);
    assert_eq!(
        state.last_error(),
        Some(&AuthoringError::Rejected("Try again".into()))
    );
    let retry = state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    assert_ne!(retry.operation_id, first.operation_id);
    state.receive(provider.handle(&retry)).unwrap();
    assert_eq!(state.projection().profiles[0].character.money, 1_070);
}

#[test]
fn profile_and_home_selection_are_explicit_and_candidates_expose_denied_placement() {
    let (mut state, _) = state_and_provider();
    assert_eq!(
        state.dispatch(AuthoringIntent::OpenHome("maya".into())),
        Err(AuthoringError::NoSelection)
    );
    state
        .dispatch(AuthoringIntent::SelectProfile("jules".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SelectCatalog("fern".into()))
        .unwrap();
    assert_eq!(
        state.draft_validity(),
        Err(AuthoringError::PermissionDenied)
    );
    assert_eq!(
        state.dispatch(AuthoringIntent::ConfirmPlacement),
        Err(AuthoringError::PermissionDenied)
    );
    assert!(state.pending().is_none());
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    assert!(state.selected_home().is_none());
    assert!(state.draft().is_none());
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SelectCatalog("coffee-table".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SetCell(GridCell { x: 7, y: 4 }))
        .unwrap();
    assert_eq!(state.draft_validity(), Err(AuthoringError::OutOfBounds));
    state.dispatch(AuthoringIntent::RotateCandidate).unwrap();
    state.draft_validity().unwrap();
    state
        .dispatch(AuthoringIntent::MoveCandidate { dx: 0, dy: 1 })
        .unwrap();
    assert_eq!(state.draft_validity(), Err(AuthoringError::OutOfBounds));
    state.dispatch(AuthoringIntent::Cancel).unwrap();
    assert!(state.draft().is_none());
}

#[test]
fn preview_shell_projector_mirrors_only_acknowledged_data_and_enables_home_through_receive() {
    let (mut state, mut provider) = state_and_provider();
    let current = preview_projection();
    let mut shell = ShellState::new(current.clone());
    let request = creation(&mut state, "Six");
    assert_eq!(shell.projection.characters.len(), 5);
    state.receive(provider.handle(&request)).unwrap();
    let projected = preview_project_authoring_to_ui(&shell.projection, state.projection()).unwrap();
    assert_eq!(projected.revision, current.revision + 1);
    assert_eq!(projected.objects, current.objects);
    assert_eq!(projected.places[0..3], current.places[0..3]);
    assert_eq!(projected.places[3].availability, Availability::Available);
    assert_eq!(shell.projection, current);
    shell
        .receive(UiEvent::ProjectionUpdated {
            projection: projected,
        })
        .unwrap();
    let created_id = state.selected_profile().unwrap().clone();
    shell
        .dispatch(UiIntent::SelectCharacter(created_id.clone()))
        .unwrap();
    shell.dispatch(UiIntent::Play).unwrap();
    assert_eq!(shell.selected_character, Some(created_id));
    assert_eq!(shell.projection.characters.len(), 6);
    own_home(&mut state);
    let request = buy(&mut state, "armchair");
    assert_eq!(shell.projection.characters[0].money, 1_250);
    state.receive(provider.handle(&request)).unwrap();
    let projection =
        preview_project_authoring_to_ui(&shell.projection, state.projection()).unwrap();
    shell
        .receive(UiEvent::ProjectionUpdated { projection })
        .unwrap();
    assert_eq!(shell.projection.characters[0].money, 1_070);
}

#[test]
fn committed_messages_round_trip_with_typed_outcome_and_projector_checks_compatibility() {
    let (mut state, mut provider) = state_and_provider();
    let request = creation(&mut state, "Six");
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<AuthoringRequest>(&encoded).unwrap(),
        request
    );
    let event = provider.handle(&request);
    let encoded = serde_json::to_string(&event).unwrap();
    assert_eq!(
        serde_json::from_str::<AuthoringEvent>(&encoded).unwrap(),
        event
    );
    let mut shell = preview_projection();
    shell.revision = u64::MAX;
    assert_eq!(
        preview_project_authoring_to_ui(&shell, provider.snapshot()),
        Err(AuthoringError::OperationLimit)
    );
    let mut shell = preview_projection();
    shell.places.retain(|p| p.id.as_ref() != "home");
    assert!(preview_project_authoring_to_ui(&shell, provider.snapshot()).is_err());
}
