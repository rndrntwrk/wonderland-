use wonderland_client_app::authoring::*;
use wonderland_contracts::Availability;
use wonderland_contracts::authoring::*;

fn pose(x: i16, y: i16, direction: Direction) -> GridPose {
    GridPose {
        cell: GridCell { x, y },
        level: 0,
        direction,
    }
}

fn request(
    projection: &AuthoringProjection,
    operation: &str,
    kind: AuthoringRequestKind,
) -> AuthoringRequest {
    AuthoringRequest {
        operation_id: operation.into(),
        base_revision: projection.revision,
        expected_sources: projection.source_revisions(&kind),
        kind,
    }
}

fn purchase(catalog_id: &str, pose: GridPose) -> AuthoringRequestKind {
    AuthoringRequestKind::BuyAndPlace {
        actor_id: "maya".into(),
        home_owner_id: "maya".into(),
        catalog_id: catalog_id.into(),
        pose,
    }
}

fn purchased_id(event: &AuthoringEvent) -> OwnedInstanceId {
    let AuthoringEvent::Committed {
        outcome: AuthoringOutcome::Purchased { instance_id, .. },
        ..
    } = event
    else {
        panic!("expected purchase commit, got {event:?}")
    };
    instance_id.clone()
}

fn assert_rejected_unchanged(
    provider: &mut PreviewAuthoringProvider,
    request: &AuthoringRequest,
    expected: AuthoringError,
) {
    let before = provider.snapshot().clone();
    let event = provider.handle(request);
    assert!(
        matches!(event, AuthoringEvent::Rejected { operation_id, base_revision, error } if operation_id == request.operation_id && base_revision == request.base_revision && error == expected)
    );
    assert_eq!(provider.snapshot(), &before);
}

#[test]
fn one_180_armchair_costs_once_and_duplicate_or_conflicting_operations_never_charge_again() {
    let mut provider = PreviewAuthoringProvider::new(preview_authoring_projection()).unwrap();
    let first = request(
        provider.snapshot(),
        "buy-1",
        purchase("armchair", pose(1, 1, Direction::North)),
    );
    let first_event = provider.handle(&first);
    let instance_id = purchased_id(&first_event);
    assert_eq!(provider.snapshot().profiles[0].character.money, 1_070);
    assert_eq!(provider.snapshot().profiles[0].home.instances.len(), 1);
    assert_eq!(
        provider.snapshot().profiles[0].home.instances[0].id,
        instance_id
    );
    assert_eq!(provider.snapshot().profiles[1].character.money, 1_250);
    let after = provider.snapshot().clone();
    assert_eq!(provider.handle(&first), first_event);
    assert_eq!(provider.snapshot(), &after);
    let mut conflict = first.clone();
    conflict.kind = purchase("bookcase", pose(2, 2, Direction::North));
    assert_rejected_unchanged(&mut provider, &conflict, AuthoringError::InvalidOperation);
    let second = request(
        provider.snapshot(),
        "buy-2",
        purchase("fern", pose(3, 3, Direction::North)),
    );
    purchased_id(&provider.handle(&second));
    assert_eq!(provider.handle(&first), first_event);
    assert_eq!(provider.snapshot().profiles[0].character.money, 1_025);
    assert_eq!(provider.snapshot().profiles[0].home.instances.len(), 2);
}

#[test]
fn stale_requests_and_exhausted_revisions_reject_without_mutating_the_provider() {
    let mut provider = PreviewAuthoringProvider::new(preview_authoring_projection()).unwrap();
    let first = request(
        provider.snapshot(),
        "buy-1",
        purchase("fern", pose(1, 1, Direction::North)),
    );
    purchased_id(&provider.handle(&first));
    let mut stale = first;
    stale.operation_id = "stale".into();
    assert_rejected_unchanged(&mut provider, &stale, AuthoringError::StaleRevision);
    let mut projection = preview_authoring_projection();
    projection.revision = u64::MAX;
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    let request = request(
        provider.snapshot(),
        "overflow",
        purchase("fern", pose(1, 1, Direction::North)),
    );
    assert_rejected_unchanged(&mut provider, &request, AuthoringError::OperationLimit);
}

#[test]
fn permission_budget_entry_bounds_and_capacity_are_rechecked_at_the_provider_boundary() {
    for (change, expected) in [
        (
            "read-only",
            AuthoringError::Unavailable("This room is read-only".into()),
        ),
        ("poor", AuthoringError::InsufficientFunds),
        ("entry", AuthoringError::EntranceReserved),
        ("outside", AuthoringError::OutOfBounds),
        ("capacity", AuthoringError::InventoryLimit),
    ] {
        let mut projection = preview_authoring_projection();
        let mut placement = pose(1, 1, Direction::North);
        match change {
            "read-only" => {
                projection.profiles[0].home.permissions.purchase = Availability::Unavailable {
                    reason: "This room is read-only".into(),
                }
            }
            "poor" => projection.profiles[0].character.money = 179,
            "entry" => placement.cell = GridCell { x: 0, y: 0 },
            "outside" => placement.cell = GridCell { x: 8, y: 2 },
            "capacity" => {
                projection.profiles[0].home.instance_capacity = CapacityPolicy::Limited { maximum: 64 };
                projection.profiles[0].home.instances = (0..64)
                    .map(|i| OwnedInstance {
                        id: format!("stored-{i}").into(),
                        catalog_id: "fern".into(),
                        placement: None,
                    })
                    .collect()
            }
            _ => unreachable!(),
        }
        let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
        let request = request(
            provider.snapshot(),
            "purchase",
            purchase("armchair", placement),
        );
        assert_rejected_unchanged(&mut provider, &request, expected);
    }
}

#[test]
fn occupied_cells_reject_and_rotating_a_two_by_one_item_changes_the_valid_boundary() {
    let mut provider = PreviewAuthoringProvider::new(preview_authoring_projection()).unwrap();
    let first = request(
        provider.snapshot(),
        "table",
        purchase("coffee-table", pose(6, 5, Direction::North)),
    );
    purchased_id(&provider.handle(&first));
    let overlap = request(
        provider.snapshot(),
        "overlap",
        purchase("armchair", pose(7, 5, Direction::North)),
    );
    assert_rejected_unchanged(&mut provider, &overlap, AuthoringError::Occupied);
    let outside = request(
        provider.snapshot(),
        "rotated-outside",
        purchase("coffee-table", pose(3, 5, Direction::East)),
    );
    assert_rejected_unchanged(&mut provider, &outside, AuthoringError::OutOfBounds);
    let rotated = request(
        provider.snapshot(),
        "rotated-fit",
        purchase("coffee-table", pose(7, 2, Direction::East)),
    );
    purchased_id(&provider.handle(&rotated));
    let overlap = request(
        provider.snapshot(),
        "rotated-overlap",
        purchase("armchair", pose(7, 3, Direction::North)),
    );
    assert_rejected_unchanged(&mut provider, &overlap, AuthoringError::Occupied);
}

#[test]
fn moving_ignores_its_own_previous_footprint_but_respects_other_instances() {
    let mut provider = PreviewAuthoringProvider::new(preview_authoring_projection()).unwrap();
    let buy_table = request(
        provider.snapshot(),
        "table",
        purchase("coffee-table", pose(1, 1, Direction::North)),
    );
    let table_id = purchased_id(&provider.handle(&buy_table));
    let move_table = request(
        provider.snapshot(),
        "move",
        AuthoringRequestKind::MoveInstance {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: table_id.clone(),
            pose: pose(2, 1, Direction::North),
        },
    );
    assert!(matches!(
        provider.handle(&move_table),
        AuthoringEvent::Committed {
            outcome: AuthoringOutcome::Moved { .. },
            ..
        }
    ));
    assert_eq!(
        provider.snapshot().profiles[0].home.instances[0].placement,
        Some(pose(2, 1, Direction::North))
    );
    assert_eq!(provider.snapshot().profiles[0].character.money, 1_130);
    let buy_chair = request(
        provider.snapshot(),
        "chair",
        purchase("armchair", pose(4, 1, Direction::North)),
    );
    purchased_id(&provider.handle(&buy_chair));
    let collision = request(
        provider.snapshot(),
        "collision",
        AuthoringRequestKind::MoveInstance {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: table_id,
            pose: pose(3, 1, Direction::North),
        },
    );
    assert_rejected_unchanged(&mut provider, &collision, AuthoringError::Occupied);
}

#[test]
fn move_cancel_retains_placement_and_store_then_place_reuses_identity_without_charge() {
    let initial = preview_authoring_projection();
    let mut provider = PreviewAuthoringProvider::new(initial.clone()).unwrap();
    let mut state = AuthoringState::new(initial);
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SelectCatalog("armchair".into()))
        .unwrap();
    let buy = state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    let event = provider.handle(&buy);
    let instance_id = purchased_id(&event);
    state.receive(event).unwrap();
    let before_move = state.projection().clone();
    state
        .dispatch(AuthoringIntent::SelectOwned(instance_id.clone()))
        .unwrap();
    state.dispatch(AuthoringIntent::BeginMove).unwrap();
    state
        .dispatch(AuthoringIntent::SetCell(GridCell { x: 4, y: 4 }))
        .unwrap();
    state.dispatch(AuthoringIntent::RotateCandidate).unwrap();
    state.dispatch(AuthoringIntent::Cancel).unwrap();
    assert_eq!(state.projection(), &before_move);
    let store = state
        .dispatch(AuthoringIntent::StoreSelected)
        .unwrap()
        .remove(0);
    assert_eq!(state.projection(), &before_move);
    state.receive(provider.handle(&store)).unwrap();
    let stored = &state.projection().profiles[0].home.instances[0];
    assert_eq!(stored.id, instance_id);
    assert_eq!(stored.placement, None);
    assert_eq!(state.projection().profiles[0].character.money, 1_070);
    state
        .dispatch(AuthoringIntent::BeginPlace(instance_id.clone()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SetCell(GridCell { x: 3, y: 2 }))
        .unwrap();
    let place = state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    state.receive(provider.handle(&place)).unwrap();
    let placed = &state.projection().profiles[0].home.instances[0];
    assert_eq!(placed.id, instance_id);
    assert_eq!(placed.placement, Some(pose(3, 2, Direction::North)));
    assert_eq!(state.projection().profiles[0].character.money, 1_070);
    assert_eq!(state.projection().profiles[0].home.instances.len(), 1);
}

#[test]
fn another_actor_cannot_spend_owner_money_or_move_store_or_place_their_instances() {
    let mut projection = preview_authoring_projection();
    projection.profiles[0].home.instances = vec![
        OwnedInstance {
            id: "stored".into(),
            catalog_id: "armchair".into(),
            placement: None,
        },
        OwnedInstance {
            id: "placed".into(),
            catalog_id: "fern".into(),
            placement: Some(pose(2, 2, Direction::North)),
        },
    ];
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    let kinds = [
        AuthoringRequestKind::BuyAndPlace {
            actor_id: "jules".into(),
            home_owner_id: "maya".into(),
            catalog_id: "armchair".into(),
            pose: pose(1, 1, Direction::North),
        },
        AuthoringRequestKind::MoveInstance {
            actor_id: "jules".into(),
            home_owner_id: "maya".into(),
            instance_id: "placed".into(),
            pose: pose(1, 1, Direction::North),
        },
        AuthoringRequestKind::StoreInstance {
            actor_id: "jules".into(),
            home_owner_id: "maya".into(),
            instance_id: "placed".into(),
        },
        AuthoringRequestKind::PlaceOwned {
            actor_id: "jules".into(),
            home_owner_id: "maya".into(),
            instance_id: "stored".into(),
            pose: pose(1, 1, Direction::North),
        },
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let request = request(provider.snapshot(), &format!("denied-{index}"), kind);
        assert_rejected_unchanged(&mut provider, &request, AuthoringError::PermissionDenied);
    }
    let request = request(
        provider.snapshot(),
        "foreign-instance",
        AuthoringRequestKind::PlaceOwned {
            actor_id: "jules".into(),
            home_owner_id: "jules".into(),
            instance_id: "stored".into(),
            pose: pose(1, 1, Direction::North),
        },
    );
    assert_rejected_unchanged(&mut provider, &request, AuthoringError::UnknownInstance);
}

#[test]
fn stored_instances_count_toward_capacity_but_can_be_placed_again_at_capacity() {
    let mut projection = preview_authoring_projection();
    projection.profiles[0].home.instance_capacity = CapacityPolicy::Limited { maximum: 64 };
    projection.profiles[0].home.instances = (0..64)
        .map(|i| OwnedInstance {
            id: format!("owned-{i}").into(),
            catalog_id: "fern".into(),
            placement: None,
        })
        .collect();
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    let place = request(
        provider.snapshot(),
        "place-existing",
        AuthoringRequestKind::PlaceOwned {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: "owned-0".into(),
            pose: pose(1, 1, Direction::North),
        },
    );
    assert!(matches!(
        provider.handle(&place),
        AuthoringEvent::Committed {
            outcome: AuthoringOutcome::Placed { .. },
            ..
        }
    ));
    assert_eq!(provider.snapshot().profiles[0].home.instances.len(), 64);
    assert_eq!(provider.snapshot().profiles[0].character.money, 1_250);
    let buy = request(
        provider.snapshot(),
        "buy-over-capacity",
        purchase("fern", pose(2, 2, Direction::North)),
    );
    assert_rejected_unchanged(&mut provider, &buy, AuthoringError::InventoryLimit);
}

#[test]
fn invalid_source_states_unavailable_items_and_arrangement_permissions_do_not_mutate() {
    let mut projection = preview_authoring_projection();
    projection.profiles[0].home.instances = vec![OwnedInstance {
        id: "stored".into(),
        catalog_id: "armchair".into(),
        placement: None,
    }];
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let move_stored = request(
        provider.snapshot(),
        "move-stored",
        AuthoringRequestKind::MoveInstance {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: "stored".into(),
            pose: pose(1, 1, Direction::North),
        },
    );
    assert_rejected_unchanged(
        &mut provider,
        &move_stored,
        AuthoringError::WrongPlacementState,
    );
    let store_stored = request(
        provider.snapshot(),
        "store-stored",
        AuthoringRequestKind::StoreInstance {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: "stored".into(),
        },
    );
    assert_rejected_unchanged(
        &mut provider,
        &store_stored,
        AuthoringError::WrongPlacementState,
    );
    projection.profiles[0].home.permissions.arrange = Availability::Unavailable {
        reason: "Arrangement is unavailable".into(),
    };
    projection.catalog[0].availability = Availability::Unavailable {
        reason: "Unavailable in this preview".into(),
    };
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    let place = request(
        provider.snapshot(),
        "place-denied",
        AuthoringRequestKind::PlaceOwned {
            actor_id: "maya".into(),
            home_owner_id: "maya".into(),
            instance_id: "stored".into(),
            pose: pose(1, 1, Direction::North),
        },
    );
    assert_rejected_unchanged(
        &mut provider,
        &place,
        AuthoringError::Unavailable("Arrangement is unavailable".into()),
    );
    let buy = request(
        provider.snapshot(),
        "buy-unavailable",
        purchase("armchair", pose(2, 2, Direction::North)),
    );
    assert_rejected_unchanged(
        &mut provider,
        &buy,
        AuthoringError::Unavailable("Unavailable in this preview".into()),
    );
}

#[test]
fn provider_rejects_invalid_construction_and_bounds_its_receipt_cache_without_reusing_operations() {
    let mut invalid = preview_authoring_projection();
    invalid.profiles[0].home.owner_id = "jules".into();
    assert!(PreviewAuthoringProvider::new(invalid).is_err());
    let mut provider = PreviewAuthoringProvider::new(preview_authoring_projection()).unwrap();
    let first = request(
        provider.snapshot(),
        "first",
        purchase("armchair", pose(1, 1, Direction::North)),
    );
    let first_event = provider.handle(&first);
    purchased_id(&first_event);
    for index in 1..MAX_PREVIEW_OPERATIONS {
        let request = request(
            provider.snapshot(),
            &format!("rejected-{index}"),
            purchase("armchair", pose(0, 0, Direction::North)),
        );
        assert_rejected_unchanged(&mut provider, &request, AuthoringError::EntranceReserved);
    }
    let overflow = request(
        provider.snapshot(),
        "overflow",
        purchase("fern", pose(3, 3, Direction::North)),
    );
    assert_rejected_unchanged(&mut provider, &overflow, AuthoringError::OperationLimit);
    assert_eq!(provider.handle(&first), first_event);
    assert_eq!(provider.snapshot().profiles[0].character.money, 1_070);
}
