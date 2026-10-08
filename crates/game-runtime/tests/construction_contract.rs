//! Trusted grants are fixture inputs; these tests do not emulate a money service.
#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_runtime::live_wire::construction::*;
use wonderland_game_runtime::live_wire::player::PlayerBinding;
use wonderland_game_runtime::sim_core::world::build::*;
use wonderland_game_runtime::*;

fn setup() -> (GameRuntime, ConstructionGrant, ConstructionRequest) {
    let (game, _, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let binding = PlayerBinding {
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 55,
        avatar_id: 7,
    };
    let mut authority = BuildAuthority::new(actor, PersistentId(7), 1000);
    authority.can_build = true;
    authority.permissions_revision = 11;
    authority.account_revision = 12;
    authority.catalog_revision = 13;
    authority.prices.floor = 7;
    authority.prices.wall_side = 10;
    authority.prices.terrain_height_step = 1;
    let grant = ConstructionGrant {
        binding,
        principal: support::PRINCIPAL,
        authority,
        floor_patterns: BTreeSet::from([1, 2]),
    };
    let state = game.sim().state();
    let request = ConstructionRequest {
        binding,
        request_id: 42,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        architecture_revision: state.world.lot.revision().architecture,
        permissions_revision: 11,
        account_revision: 12,
        catalog_revision: 13,
        selections: vec![Selection::Floor {
            tile: TilePos::new(2, 2, 1),
            pattern: 1,
        }],
    };
    (game, grant, request)
}
fn event_request(out: &TickOutcome) -> DurableBuildRequest {
    out.events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::BuildRequested(r) = e {
                Some(r.clone())
            } else {
                None
            }
        })
        .expect("accepted begin requests durable completion")
}
fn completion(
    request: &DurableBuildRequest,
    status: DurableBuildStatus,
) -> ServerBuildConfirmation {
    let committed = matches!(status, DurableBuildStatus::Committed);
    ServerBuildConfirmation {
        operation: request.operation,
        actor: request.actor,
        owner: request.owner,
        preview_hash: request.preview_hash,
        charged_cost: if committed { request.cost } else { 0 },
        durable_receipt: if committed { 99 } else { 0 },
        status,
        created_objects: BTreeMap::new(),
    }
}
#[test]
fn quotes_are_read_only_and_use_server_prices_and_operation_identity() {
    let (game, grant, request) = setup();
    let before = game.snapshot().unwrap();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    assert_eq!(offer.request_id(), 42);
    assert_eq!(offer.preview().intent.operation, 900);
    assert_eq!(offer.preview().cost, 7);
    assert_eq!(offer.preview().owner, PersistentId(7));
    assert_eq!(game.snapshot().unwrap(), before);
}
#[test]
fn consent_begin_and_durable_completion_remain_three_distinct_steps() {
    let (mut game, grant, request) = setup();
    let tile = TilePos::new(2, 2, 1);
    let offer = quote(&game, &grant, 900, &request).unwrap();
    let before = game.snapshot().unwrap();
    let command = confirm(&game, &grant, &offer, offer.consent()).unwrap();
    assert_eq!(game.snapshot().unwrap(), before);
    assert!(matches!(command, AcceptedCommand::BeginBuild { .. }));
    let out = game.advance(vec![command]).unwrap();
    let durable = event_request(&out);
    assert_eq!(game.sim().state().world.lot.tile(tile).unwrap().floor, 0);
    game.advance(vec![AcceptedCommand::CompleteBuild(completion(
        &durable,
        DurableBuildStatus::Committed,
    ))])
    .unwrap();
    assert_eq!(game.sim().state().world.lot.tile(tile).unwrap().floor, 1);
    let revision = game.sim().state().world.lot.revision();
    game.advance(vec![AcceptedCommand::CompleteBuild(completion(
        &durable,
        DurableBuildStatus::Committed,
    ))])
    .unwrap();
    assert_eq!(game.sim().state().world.lot.revision(), revision);
}
#[test]
fn rejected_durable_completion_does_not_change_geometry() {
    let (mut game, grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    let out = game
        .advance(vec![
            confirm(&game, &grant, &offer, offer.consent()).unwrap(),
        ])
        .unwrap();
    let durable = event_request(&out);
    game.advance(vec![AcceptedCommand::CompleteBuild(completion(
        &durable,
        DurableBuildStatus::Rejected {
            reason: BuildError::InsufficientFunds,
        },
    ))])
    .unwrap();
    assert_eq!(
        game.sim()
            .state()
            .world
            .lot
            .tile(TilePos::new(2, 2, 1))
            .unwrap()
            .floor,
        0
    );
}
#[test]
fn ordinary_ticks_do_not_expire_unchanged_construction_quotes() {
    let (mut game, grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    for _ in 0..4 {
        game.advance(vec![]).unwrap();
    }
    assert!(confirm(&game, &grant, &offer, offer.consent()).is_ok());
}
#[test]
fn wrong_hash_request_or_price_cannot_authorize_a_build() {
    let (game, grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    for field in 0..3 {
        let mut consent = offer.consent();
        match field {
            0 => consent.preview_hash[0] ^= 1,
            1 => consent.cost += 1,
            _ => consent.request_id += 1,
        };
        assert!(confirm(&game, &grant, &offer, consent).is_err());
    }
}
#[test]
fn permission_account_catalog_and_balance_changes_expire_offers() {
    let (game, grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    for field in 0..6 {
        let mut changed = grant.clone();
        match field {
            0 => changed.authority.permissions_revision += 1,
            1 => changed.authority.account_revision += 1,
            2 => changed.authority.catalog_revision += 1,
            3 => changed.authority.can_build = false,
            4 => changed.authority.balance = 0,
            _ => changed.authority.prices.floor += 1,
        };
        assert!(
            confirm(&game, &changed, &offer, offer.consent()).is_err(),
            "field{field}"
        );
    }
}
#[test]
fn client_identity_and_revision_substitution_is_rejected() {
    let (game, grant, request) = setup();
    for field in 0..9 {
        let mut r = request.clone();
        match field {
            0 => r.binding.source_epoch += 1,
            1 => r.binding.avatar_id += 1,
            2 => r.binding.lot_incarnation += 1,
            3 => r.binding.lot_location += 1,
            4 => r.lot_id += 1,
            5 => r.authority_epoch += 1,
            6 => r.architecture_revision += 1,
            7 => r.permissions_revision += 1,
            _ => r.account_revision += 1,
        };
        assert!(quote(&game, &grant, 900, &r).is_err(), "field{field}");
    }
}
#[test]
fn authenticated_principal_and_actor_generation_are_checked() {
    let (game, grant, request) = setup();
    for field in 0..4 {
        let mut g = grant.clone();
        match field {
            0 => g.principal = PrincipalKey(999),
            1 => g.authority.actor.generation += 1,
            2 => g.authority.owner = PersistentId(8),
            _ => g.authority.connected = false,
        };
        assert!(quote(&game, &g, 900, &request).is_err());
    }
}
#[test]
fn revoking_the_runtime_grant_invalidates_an_already_quoted_edit() {
    let (mut game, grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    game.advance(vec![AcceptedCommand::SetInteractionAuthority {
        actor: grant.authority.actor,
        access: None,
    }])
    .unwrap();
    assert!(confirm(&game, &grant, &offer, offer.consent()).is_err());
}
#[test]
fn unknown_materials_bad_geometry_empty_edits_and_zero_ids_reject() {
    let (game, grant, request) = setup();
    for selections in [
        vec![],
        vec![Selection::Floor {
            tile: TilePos::new(2, 2, 1),
            pattern: 99,
        }],
        vec![Selection::Floor {
            tile: TilePos::new(-1, 2, 1),
            pattern: 1,
        }],
        vec![Selection::SolidWall {
            tile: TilePos::new(2, 2, 1),
            sides: 32,
        }],
        vec![Selection::Terrain {
            x: 9,
            y: 0,
            height: 100,
        }],
    ] {
        let r = ConstructionRequest {
            selections,
            ..request.clone()
        };
        assert!(quote(&game, &grant, 900, &r).is_err());
    }
    assert!(quote(&game, &grant, 0, &request).is_err());
    assert!(
        quote(
            &game,
            &grant,
            900,
            &ConstructionRequest {
                request_id: 0,
                ..request
            }
        )
        .is_err()
    );
}
#[test]
fn server_material_withdrawal_invalidates_previously_reviewed_quote() {
    let (game, mut grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    grant.floor_patterns.clear();
    assert!(confirm(&game, &grant, &offer, offer.consent()).is_err());
}
#[test]
fn actor_cannot_be_moved_or_removed_as_a_purchased_object() {
    let (game, grant, request) = setup();
    let entity = grant.authority.actor;
    let revision = game.sim().state().world.object(entity).unwrap().revision;
    for selection in [
        Selection::Move {
            entity,
            expected_revision: revision,
            position: TilePos::new(5, 5, 1).center(),
            facing: Facing::EAST,
        },
        Selection::Remove {
            entity,
            expected_revision: revision,
        },
    ] {
        let r = ConstructionRequest {
            selections: vec![selection],
            ..request.clone()
        };
        assert!(quote(&game, &grant, 900, &r).is_err());
    }
}
#[test]
fn wall_and_terrain_choices_use_the_existing_native_quote_engine() {
    let (game, grant, request) = setup();
    for (selection, cost) in [
        (
            Selection::SolidWall {
                tile: TilePos::new(2, 2, 1),
                sides: 1,
            },
            10,
        ),
        (
            Selection::Terrain {
                x: 0,
                y: 0,
                height: 4,
            },
            4,
        ),
    ] {
        let r = ConstructionRequest {
            selections: vec![selection],
            ..request.clone()
        };
        assert_eq!(quote(&game, &grant, 900, &r).unwrap().preview().cost, cost);
    }
}
#[test]
fn request_binary_roundtrip_preserves_full_width_ids() {
    let (_, _, mut request) = setup();
    request.request_id = u64::MAX;
    request.lot_id = u64::MAX - 1;
    request.authority_epoch = 9_007_199_254_740_993;
    let bytes = encode_request(&request).unwrap();
    assert_eq!(decode_request(&bytes).unwrap(), request);
    for len in 0..bytes.len() {
        assert!(decode_request(&bytes[..len]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode_request(&extra).is_err());
    for at in [0, 4, 8, 15] {
        let mut bad = bytes.clone();
        bad[at] ^= 0xff;
        assert!(decode_request(&bad).is_err());
    }
}
#[test]
fn decoder_rejects_hostile_collection_hints_before_unbounded_reservation() {
    let (_, _, mut request) = setup();
    request.selections.clear();
    let mut body = bincode::serialize(&request).unwrap();
    let n = body.len();
    body[n - 8..].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut bytes = b"WLCB\x01\r\n\x1a".to_vec();
    bytes.extend_from_slice(&(body.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&body);
    assert!(decode_request(&bytes).is_err());
}

fn shop() -> (GameRuntime, ConstructionGrant, ConstructionRequest) {
    use wonderland_game_runtime::sim_core::state::{ContentSet, ObjectDefinition};
    let original = support::content("cursebook_set_permission.iff", 4107, false);
    let mut object = ObjectDefinition::new(123, 0);
    object.object_data[42] = 3;
    object.object_data[4] = 1;
    let content = ContentSet::new(
        original.routines().clone(),
        vec![original.object(support::OWNER).unwrap().clone(), object],
        vec![],
        original.tuning().clone(),
    )
    .unwrap()
    .with_interaction_tables(vec![(
        support::OWNER,
        original.interaction_table(support::OWNER).unwrap().clone(),
    )])
    .unwrap()
    .with_build_catalog(vec![(987, 123)])
    .unwrap();
    let (game, _, actor) = support::pair_with_content(content, true);
    let (_, mut grant, mut request) = setup();
    assert_eq!(grant.authority.actor, actor);
    grant.authority.prices.objects.insert(987, 200);
    request.selections = vec![Selection::Purchase {
        catalog_id: 987,
        position: TilePos::new(5, 5, 1).center(),
        facing: Facing::NORTH,
    }];
    (game, grant, request)
}
fn buy(
    game: &mut GameRuntime,
    grant: &ConstructionGrant,
    request: &ConstructionRequest,
    operation: u64,
) -> EntityRef {
    let offer = quote(game, grant, operation, request).unwrap();
    let reference = match &offer.preview().intent.edits[0] {
        BuildEdit::PlaceObject { object, .. } => object.entity,
        _ => panic!("purchase"),
    };
    let command = confirm(game, grant, &offer, offer.consent()).unwrap();
    let request = event_request(&game.advance(vec![command]).unwrap());
    let mut receipt = completion(&request, DurableBuildStatus::Committed);
    receipt
        .created_objects
        .insert(reference, PersistentId(9000 + operation as u32));
    game.advance(vec![AcceptedCommand::CompleteBuild(receipt)])
        .unwrap();
    reference
}
#[test]
fn purchase_quote_derives_real_source_object_and_never_reserves_before_confirmation() {
    let (mut game, grant, request) = shop();
    let before = game.snapshot().unwrap();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    assert_eq!(game.snapshot().unwrap(), before);
    assert_eq!(offer.preview().cost, 200);
    let BuildEdit::PlaceObject { catalog_id, object } = &offer.preview().intent.edits[0] else {
        panic!("place")
    };
    assert_eq!(*catalog_id, 987);
    assert_eq!(object.owner, Some(PersistentId(7)));
    let reference = object.entity;
    let command = confirm(&game, &grant, &offer, offer.consent()).unwrap();
    assert!(!game.sim().state().ids.is_live(reference));
    let durable = event_request(&game.advance(vec![command]).unwrap());
    assert!(!game.sim().state().ids.is_live(reference));
    let mut receipt = completion(&durable, DurableBuildStatus::Committed);
    receipt.created_objects.insert(reference, PersistentId(999));
    game.advance(vec![AcceptedCommand::CompleteBuild(receipt.clone())])
        .unwrap();
    assert_eq!(
        game.sim().state().entities[&reference.object_id].info.guid,
        123
    );
    assert_eq!(
        game.sim().state().entities[&reference.object_id]
            .info
            .persistent_id,
        999
    );
    assert_eq!(
        game.sim().state().world.object(reference).unwrap().owner,
        Some(PersistentId(7))
    );
    assert!(
        game.sim()
            .state()
            .threads
            .contains_key(&reference.object_id)
    );
    let out = game
        .advance(vec![AcceptedCommand::CompleteBuild(receipt)])
        .unwrap();
    assert!(
        !out.events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::Spawned(_)))
    );
    assert_eq!(game.sim().state().entities.len(), 2);
}
#[test]
fn multiple_purchases_have_unique_native_reservations_and_one_exact_quote() {
    let (mut game, grant, mut request) = shop();
    request.selections.push(Selection::Purchase {
        catalog_id: 987,
        position: TilePos::new(6, 5, 1).center(),
        facing: Facing::EAST,
    });
    let before = game.snapshot().unwrap();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    assert_eq!(offer.preview().cost, 400);
    let refs = offer
        .preview()
        .intent
        .edits
        .iter()
        .map(|e| match e {
            BuildEdit::PlaceObject { object, .. } => object.entity,
            _ => panic!(),
        })
        .collect::<Vec<_>>();
    assert_ne!(refs[0], refs[1]);
    assert_eq!(game.snapshot().unwrap(), before);
    let command = confirm(&game, &grant, &offer, offer.consent()).unwrap();
    let durable = event_request(&game.advance(vec![command]).unwrap());
    let mut receipt = completion(&durable, DurableBuildStatus::Committed);
    for (i, reference) in refs.iter().enumerate() {
        receipt
            .created_objects
            .insert(*reference, PersistentId(999 + i as u32));
    }
    game.advance(vec![AcceptedCommand::CompleteBuild(receipt)])
        .unwrap();
    assert_eq!(game.sim().state().entities.len(), 3);
}
#[test]
fn catalogue_identity_price_and_budget_are_not_supplied_by_the_purchase_request() {
    let (game, mut grant, mut request) = shop();
    grant.authority.balance = 199;
    assert!(quote(&game, &grant, 900, &request).is_err());
    grant.authority.balance = 1000;
    grant.authority.prices.objects.remove(&987);
    assert!(quote(&game, &grant, 900, &request).is_err());
    grant.authority.prices.objects.insert(555, 1);
    request.selections[0] = Selection::Purchase {
        catalog_id: 555,
        position: TilePos::new(5, 5, 1).center(),
        facing: Facing::NORTH,
    };
    assert!(quote(&game, &grant, 900, &request).is_err());
}
#[test]
fn allocation_between_quote_and_confirmation_requires_new_consent() {
    let (mut game, grant, request) = shop();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    game.advance(vec![AcceptedCommand::Spawn(SpawnSpec {
        guid: 123,
        position: TilePos::new(6, 6, 1).center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(600),
        avatar: false,
    })])
    .unwrap();
    assert!(confirm(&game, &grant, &offer, offer.consent()).is_err());
}
#[test]
fn purchased_object_move_and_removal_follow_durable_acceptance() {
    let (mut game, grant, mut request) = shop();
    let reference = buy(&mut game, &grant, &request, 900);
    let object = game.sim().state().world.object(reference).unwrap();
    let old = object.position;
    let destination = TilePos::new(6, 5, 1).center();
    request.request_id += 1;
    request.architecture_revision = game.sim().state().world.lot.revision().architecture;
    request.selections = vec![Selection::Move {
        entity: reference,
        expected_revision: object.revision,
        position: destination,
        facing: Facing::EAST,
    }];
    let offer = quote(&game, &grant, 901, &request).unwrap();
    let command = confirm(&game, &grant, &offer, offer.consent()).unwrap();
    let durable = event_request(&game.advance(vec![command]).unwrap());
    assert_eq!(
        game.sim().state().world.object(reference).unwrap().position,
        old
    );
    game.advance(vec![AcceptedCommand::CompleteBuild(completion(
        &durable,
        DurableBuildStatus::Committed,
    ))])
    .unwrap();
    assert_eq!(
        game.sim().state().world.object(reference).unwrap().position,
        destination
    );
    assert_eq!(
        game.projection()
            .entities
            .iter()
            .find(|e| e.reference == reference)
            .unwrap()
            .position,
        destination
    );
    request.request_id += 1;
    request.architecture_revision = game.sim().state().world.lot.revision().architecture;
    request.selections = vec![Selection::Remove {
        entity: reference,
        expected_revision: game.sim().state().world.object(reference).unwrap().revision,
    }];
    let offer = quote(&game, &grant, 902, &request).unwrap();
    let command = confirm(&game, &grant, &offer, offer.consent()).unwrap();
    let durable = event_request(&game.advance(vec![command]).unwrap());
    assert!(game.sim().state().ids.is_live(reference));
    game.advance(vec![AcceptedCommand::CompleteBuild(completion(
        &durable,
        DurableBuildStatus::Committed,
    ))])
    .unwrap();
    assert!(!game.sim().state().ids.is_live(reference));
    assert!(game.sim().state().world.object(reference).is_none());
}
#[test]
fn ownership_and_object_generation_are_rechecked_for_move_and_remove() {
    let (mut game, mut grant, mut request) = shop();
    let reference = buy(&mut game, &grant, &request, 900);
    request.architecture_revision = game.sim().state().world.lot.revision().architecture;
    let revision = game.sim().state().world.object(reference).unwrap().revision;
    for invalid in [
        EntityRef {
            generation: reference.generation + 1,
            ..reference
        },
        grant.authority.actor,
    ] {
        request.selections = vec![Selection::Remove {
            entity: invalid,
            expected_revision: revision,
        }];
        assert!(quote(&game, &grant, 901, &request).is_err());
    }
    request.selections = vec![Selection::Remove {
        entity: reference,
        expected_revision: revision + 1,
    }];
    assert!(quote(&game, &grant, 901, &request).is_err());
    // An authenticated user grant for a different payer cannot claim this object.
    grant.authority.owner = PersistentId(99);
    request.selections = vec![Selection::Remove {
        entity: reference,
        expected_revision: revision,
    }];
    assert!(quote(&game, &grant, 901, &request).is_err());
}
#[test]
fn both_maximum_request_and_one_over_limit_are_checked() {
    let (_, _, mut request) = setup();
    request.selections = vec![request.selections[0].clone(); MAX_BUILD_EDITS];
    let bytes = encode_request(&request).unwrap();
    assert_eq!(decode_request(&bytes).unwrap(), request);
    request.selections.push(request.selections[0].clone());
    assert!(encode_request(&request).is_err());
}
#[test]
fn replica_cannot_issue_authority_construction_quotes() {
    let (game, grant, request) = setup();
    let mut replica = GameRuntime::new(
        game.sim().content().clone(),
        game.sim().state().world.lot.clone(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Replica,
    )
    .unwrap();
    replica.restore(&game.snapshot().unwrap()).unwrap();
    assert_eq!(
        quote(&replica, &grant, 900, &request),
        Err(ConstructionError::NotAuthority)
    );
}

#[test]
fn quote_cannot_transfer_to_a_different_authenticated_principal() {
    let (mut game, mut grant, request) = setup();
    let offer = quote(&game, &grant, 900, &request).unwrap();
    // Even if a caller reuses the same binding/quote store key, consent belonged
    // to the original principal. The server must issue a fresh quote for the new one.
    grant.principal = PrincipalKey(99);
    game.advance(vec![AcceptedCommand::SetInteractionAuthority {
        actor: grant.authority.actor,
        access: Some(InteractionAccess {
            principal: grant.principal,
            allow_hidden: false,
        }),
    }])
    .unwrap();
    assert!(confirm(&game, &grant, &offer, offer.consent()).is_err());
}
