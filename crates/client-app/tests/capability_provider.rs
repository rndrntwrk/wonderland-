use wonderland_client_app::authoring::*;
use wonderland_contracts::authoring::*;
use wonderland_contracts::Availability;

fn supplied_projection() -> AuthoringProjection {
    let mut projection = preview_authoring_projection();
    projection.catalog_categories.push(CatalogCategory { id: "music".into(), label: "Music".into() });
    projection.catalog.push(CatalogItem {
        id: "source-piano".into(),
        source_key: "00000000A9710000:00000001".into(),
        name: "Source piano".into(),
        category: "music".into(),
        price: 4200,
        footprint: Footprint { width: 3, depth: 2 },
        rotations: vec![Direction::West, Direction::East],
        thumbnail: None,
        availability: Availability::Available,
    });
    projection.catalog_revision = 11;
    projection.appearance_content.revision = 7;
    projection.account.revision = 13;
    projection.profiles[0].character.money = 9_876_543;
    projection.profiles[0].home.lot.bounds.width = 64;
    projection.profiles[0].home.lot.bounds.depth = 48;
    projection.profiles[0].home.lot.levels = vec![0, 3];
    projection
}

#[test]
fn a_custom_purchase_uses_supplied_price_rotations_bounds_level_and_source_revisions() {
    let projection = supplied_projection();
    let mut state = AuthoringState::new(projection.clone());
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    state.dispatch(AuthoringIntent::SelectProfile("maya".into())).unwrap();
    state.dispatch(AuthoringIntent::OpenHome("maya".into())).unwrap();
    state.dispatch(AuthoringIntent::SelectCatalog("source-piano".into())).unwrap();
    state.dispatch(AuthoringIntent::SetCell(GridCell { x: 31, y: 42 })).unwrap();
    state.dispatch(AuthoringIntent::SetLevel(3)).unwrap();
    state.dispatch(AuthoringIntent::RotateCandidate).unwrap();
    let request = state.dispatch(AuthoringIntent::ConfirmPlacement).unwrap().remove(0);
    assert_eq!(request.expected_sources.account, 13);
    assert_eq!(request.expected_sources.appearance, 7);
    assert_eq!(request.expected_sources.catalog, 11);
    assert_eq!(request.expected_sources.lot, Some(1));
    let mut stale = request.clone();
    stale.operation_id = "stale-content".into();
    stale.expected_sources.catalog = 10;
    assert!(matches!(provider.handle(&stale), AuthoringEvent::Rejected { error: AuthoringError::StaleRevision, .. }));
    assert_eq!(provider.snapshot(), &projection);
    state.receive(provider.handle(&request)).unwrap();
    let profile = &state.projection().profiles[0];
    assert_eq!(profile.character.money, 9_872_343);
    let placement = profile.home.instances[0].placement.unwrap();
    assert_eq!(placement.cell, GridCell { x: 31, y: 42 });
    assert_eq!(placement.level, 3);
    assert_eq!(placement.direction, Direction::East);
    assert_eq!(profile.home.lot.revision, 2);
    assert_eq!(profile.home.instances[0].catalog_id.as_ref(), "source-piano");
}

#[test]
fn supplied_creation_capacity_allows_a_twelfth_profile_and_saved_overflow_stays_readable() {
    let mut projection = preview_authoring_projection();
    for index in 5..11 {
        let mut profile = projection.profiles[0].clone();
        profile.character.id = format!("saved-{index}").into();
        profile.home.owner_id = profile.character.id.clone();
        projection.profiles.push(profile);
    }
    projection.account.profile_capacity = CapacityPolicy::Limited { maximum: 12 };
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection);
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state.dispatch(AuthoringIntent::UpdateName("Twelfth".into())).unwrap();
    let request = state.dispatch(AuthoringIntent::SubmitCreate).unwrap().remove(0);
    state.receive(provider.handle(&request)).unwrap();
    assert_eq!(state.projection().profiles.len(), 12);
    assert_eq!(state.dispatch(AuthoringIntent::OpenCreate), Err(AuthoringError::ProfileLimit));
    let mut legacy = state.projection().clone();
    legacy.account = legacy_account_capabilities();
    legacy.validate().unwrap();
    assert_eq!(legacy.profiles.len(), 12);
    assert_eq!(AuthoringState::new(legacy).dispatch(AuthoringIntent::OpenCreate), Err(AuthoringError::ProfileLimit));
}

#[test]
fn unknown_account_capacity_does_not_grant_unlimited_creation() {
    let mut projection = preview_authoring_projection();
    projection.account.profile_capacity = CapacityPolicy::Unknown;
    let before = projection.clone();
    let mut state = AuthoringState::new(projection);
    assert!(matches!(state.dispatch(AuthoringIntent::OpenCreate), Err(AuthoringError::Unavailable(_))));
    assert_eq!(state.projection(), &before);
    assert!(state.draft().is_none());
}

#[test]
fn created_profile_initial_balance_can_be_supplied_by_the_accepting_service() {
    let projection = preview_authoring_projection();
    let mut state = AuthoringState::new(projection.clone());
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state.dispatch(AuthoringIntent::UpdateName("Source account".into())).unwrap();
    let request = state.dispatch(AuthoringIntent::SubmitCreate).unwrap().remove(0);
    let mut event = provider.handle(&request);
    if let AuthoringEvent::Committed { projection, outcome: AuthoringOutcome::ProfileCreated { character_id }, .. } = &mut event {
        let profile = projection.profiles.iter_mut().find(|profile| profile.character.id == *character_id).unwrap();
        profile.character.money = 50_000;
        profile.home.lot.bounds.width = 64;
        profile.home.lot.bounds.depth = 48;
    } else {
        panic!("expected creation receipt");
    }
    state.receive(event).unwrap();
    let profile = state.projection().profile(state.selected_profile().unwrap()).unwrap();
    assert_eq!(profile.character.money, 50_000);
    assert_eq!(profile.home.lot.bounds.width, 64);
}

#[test]
fn architecture_capability_can_describe_a_request_but_missing_runtime_remains_unavailable() {
    let projection = preview_authoring_projection();
    let home = &projection.profiles[0].home;
    let request = ArchitectureRequest {
        operation_id: "build-preview-1".into(),
        base_revision: projection.revision,
        lot_revision: home.lot.revision,
        build_revision: home.build.revision,
        actor_id: "maya".into(), home_owner_id: "maya".into(),
        phase: ArchitecturePhase::Preview,
        edit: ArchitectureEdit {
            tool_id: "walls".into(), content_key: None,
            from: LotCell { cell: GridCell { x: 1, y: 1 }, level: 0 },
            to: LotCell { cell: GridCell { x: 3, y: 1 }, level: 0 },
            direction: Direction::North, value: None,
        },
    };
    assert!(matches!(request.validate(&projection), Err(AuthoringError::Unavailable(_))));
}
