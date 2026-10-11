use wonderland_player_authoring::*;
fn actor() -> SourceActorLot {
    SourceActorLot {
        avatar_id: 42,
        lot_id: Some(77),
        location: 0x00b80148,
        epoch: 5,
        incarnation: 9,
    }
}
fn outfit(id: u32, category: u8) -> OwnedOutfit {
    OwnedOutfit {
        outfit_id: id,
        asset_id: u64::MAX - id as u64,
        sale_price: 1000,
        purchase_price: 500,
        owner_type: 1,
        owner_id: 42,
        category,
        source: 0,
        label: None,
        thumbnail: None,
    }
}
fn snapshot() -> AuthoringSnapshot {
    AuthoringSnapshot {
        actor: actor(),
        revision: 10,
        permission: Some(3),
        community: false,
        bounds: Some(LotBounds {
            width: 64,
            height: 64,
            levels: 5,
        }),
        budget: Some(123456789),
        can_place_user: true,
        can_place_donated: true,
        build_resources: vec![],
        outfits_loaded: true,
        catalog: vec![CatalogItem {
            guid: 100,
            category: 12,
            price: 999999,
            name: "Real catalog".into(),
            disable_level: 0,
            tags: None,
            thumbnail: None,
        }],
        inventory: vec![InventoryItem {
            persist_id: 300,
            owner_id: 42,
            guid: 100,
            name: "Owned".into(),
            value: 888888,
            thumbnail: None,
        }],
        objects: vec![SourceObject {
            entity: EntityIdentity {
                object_id: 123,
                incarnation: 7,
            },
            persist_id: 400,
            guid: 100,
            name: "Actual object".into(),
            owner_id: 42,
            donated: false,
            is_avatar: false,
            movable: Some(true),
            placement: None,
            transaction_incomplete: false,
        }],
        outfits: vec![outfit(1, 0), outfit(2, 0), outfit(3, 8)],
        defaults: Default::default(),
        eod: Some(EodSession {
            incarnation: 22,
            plugin_id: DRESSER_PLUGIN,
            actor_id: 42,
            object_pid: Some(400),
            object_owner_id: Some(42),
            rack_type: None,
        }),
    }
}
fn state() -> AuthoringState {
    let mut state = AuthoringState::default();
    state.install(snapshot()).unwrap();
    state
}
#[test]
fn dresser_uses_owned_record_id_and_original_category_for_default() {
    let mut s = state();
    let request = s
        .submit(AuthoringIntent::SetDefault { outfit_id: 2 })
        .unwrap();
    assert_eq!(
        request.wire,
        AuthoringWire::Eod {
            incarnation: 22,
            plugin_id: DRESSER_PLUGIN,
            event_name: "dresser_set_default".into(),
            text: "0,2".into()
        }
    );
    assert_eq!(s.snapshot.as_ref().unwrap().outfits.len(), 3);
    assert_eq!(s.snapshot.as_ref().unwrap().budget, Some(123456789));
}
#[test]
fn refuses_foreign_outfit_last_clothing_deletion_and_stale_eod_owner() {
    let mut s = state();
    assert!(s.submit(AuthoringIntent::Wear { outfit_id: 99 }).is_err());
    s.snapshot
        .as_mut()
        .unwrap()
        .outfits
        .retain(|o| o.outfit_id != 2);
    assert!(
        s.submit(AuthoringIntent::DeleteOutfit { outfit_id: 1 })
            .is_err()
    );
    assert!(
        s.submit(AuthoringIntent::SetDefault { outfit_id: 3 })
            .is_err()
    );
    s.snapshot.as_mut().unwrap().eod.as_mut().unwrap().actor_id = 77;
    assert!(s.submit(AuthoringIntent::Wear { outfit_id: 1 }).is_err());
}
#[test]
fn request_revalidates_entity_incarnation_inventory_owner_and_permission() {
    let mut s = state();
    assert!(
        s.submit(AuthoringIntent::Move {
            entity: EntityIdentity {
                object_id: 123,
                incarnation: 8
            },
            placement: Placement {
                x: 16,
                y: 32,
                level: 1,
                direction: 1
            }
        })
        .is_err()
    );
    s.snapshot.as_mut().unwrap().inventory[0].owner_id = 99;
    assert!(
        s.submit(AuthoringIntent::PlaceInventory {
            persist_id: 300,
            placement: Placement {
                x: 16,
                y: 32,
                level: 1,
                direction: 1
            },
            desired_mode: 1
        })
        .is_err()
    );
    s.snapshot.as_mut().unwrap().permission = Some(0);
    assert!(
        s.submit(AuthoringIntent::Architecture {
            commands: vec![ArchitectureCommand {
                kind: 5,
                x: 1,
                y: 2,
                level: 1,
                x2: 2,
                y2: 3,
                pattern: 44,
                style: 0
            }]
        })
        .is_err()
    );
}
#[test]
fn unknown_receipt_retains_draft_and_blocks_duplicate_costs() {
    let mut s = state();
    let request = s.submit(AuthoringIntent::Wear { outfit_id: 2 }).unwrap();
    s.receive(AuthoringReceipt::Unknown {
        operation_id: request.operation_id,
        actor: actor(),
        message: "Awaiting source tick".into(),
    })
    .unwrap();
    assert_eq!(
        s.status,
        OperationState::Unknown("Awaiting source tick".into())
    );
    assert!(s.pending.is_some());
    assert!(s.draft.is_some());
    assert_eq!(
        s.submit(AuthoringIntent::Wear { outfit_id: 1 }),
        Err(AuthoringError::Pending)
    );
    assert_eq!(s.snapshot.as_ref().unwrap().budget, Some(123456789));
}
#[test]
fn rejection_preserves_draft_and_accepted_projection_is_atomic() {
    let mut s = state();
    let request = s
        .submit(AuthoringIntent::DeleteOutfit { outfit_id: 2 })
        .unwrap();
    s.receive(AuthoringReceipt::Rejected {
        operation_id: request.operation_id,
        actor: actor(),
        message: "Busy".into(),
    })
    .unwrap();
    assert!(s.draft.is_some());
    assert_eq!(s.snapshot.as_ref().unwrap().outfits.len(), 3);
    let request = s
        .submit(AuthoringIntent::DeleteOutfit { outfit_id: 2 })
        .unwrap();
    let mut accepted = snapshot();
    accepted.revision = 11;
    accepted.outfits.retain(|o| o.outfit_id != 2);
    accepted.budget = Some(123450000);
    s.receive(AuthoringReceipt::Accepted {
        operation_id: request.operation_id,
        actor: actor(),
        snapshot: Box::new(accepted),
        amount: Some(-6789),
    })
    .unwrap();
    assert!(s.pending.is_none());
    assert!(s.draft.is_none());
    assert_eq!(s.snapshot.as_ref().unwrap().budget, Some(123450000));
    assert_eq!(
        s.status,
        OperationState::Accepted {
            amount: Some(-6789)
        }
    );
}
#[test]
fn stale_receipts_cannot_mutate_new_lot_incarnation_or_older_revision() {
    let mut s = state();
    let request = s.submit(AuthoringIntent::Wear { outfit_id: 1 }).unwrap();
    let mut bad = snapshot();
    bad.revision = 9;
    assert!(
        s.receive(AuthoringReceipt::Accepted {
            operation_id: request.operation_id,
            actor: actor(),
            snapshot: Box::new(bad),
            amount: None
        })
        .is_err()
    );
    let mut next = snapshot();
    next.actor.incarnation = 10;
    s.install(next).unwrap();
    assert!(
        s.receive(AuthoringReceipt::Rejected {
            operation_id: request.operation_id,
            actor: actor(),
            message: "Old lot".into()
        })
        .is_err()
    );
    assert_eq!(s.snapshot.unwrap().actor.incarnation, 10);
}
#[test]
fn original_default_and_community_purchase_delete_modes_are_preserved() {
    assert_eq!(source_purchase_mode(1, false, 12, 2, false).unwrap(), 1);
    assert!(source_purchase_mode(1, false, 0, 1, false).is_err());
    assert_eq!(source_purchase_mode(2, true, 0, 1, false).unwrap(), 2);
    assert_eq!(source_purchase_mode(3, true, 0, 1, false).unwrap(), 1);
    let mut obj = snapshot().objects.remove(0);
    obj.owner_id = 99;
    assert_eq!(
        source_delete_mode(42, 1, false, &obj, Some(12), 2).unwrap(),
        1
    );
    assert!(source_delete_mode(42, 2, true, &obj, Some(12), 2).is_err());
    obj.donated = true;
    assert_eq!(
        source_delete_mode(42, 2, true, &obj, Some(0), 2).unwrap(),
        2
    );
    assert!(source_delete_mode(42, 2, true, &obj, Some(0), 1).is_err());
}
#[test]
fn architecture_families_reject_visitor_and_preserve_draft_after_source_rejection() {
    for kind in 0..10 {
        let intent = AuthoringIntent::Architecture {
            commands: vec![ArchitectureCommand {
                kind,
                x: 1,
                y: 2,
                level: 1,
                x2: 3,
                y2: 4,
                pattern: 65000,
                style: 64000,
            }],
        };
        let mut s = state();
        s.snapshot.as_mut().unwrap().permission = Some(0);
        assert!(s.submit(intent.clone()).is_err());
        s.snapshot.as_mut().unwrap().permission = Some(2);
        let request = s.submit(intent.clone()).unwrap();
        s.receive(AuthoringReceipt::Rejected {
            operation_id: request.operation_id,
            actor: actor(),
            message: "Source placement conflict".into(),
        })
        .unwrap();
        assert_eq!(s.draft, Some(intent));
        assert_eq!(s.snapshot.as_ref().unwrap().budget, Some(123456789));
    }
}
#[test]
fn rack_purchase_uses_original_boolean_text_not_numeric_flags() {
    let mut snapshot = snapshot();
    snapshot.eod = Some(EodSession {
        incarnation: 44,
        plugin_id: RACK_CUSTOMER_PLUGIN,
        actor_id: 42,
        object_pid: Some(400),
        object_owner_id: None,
        rack_type: None,
    });
    snapshot.outfits = vec![OwnedOutfit {
        owner_type: 2,
        owner_id: 400,
        ..outfit(1, 0)
    }];
    assert_eq!(
        snapshot
            .prepare(&AuthoringIntent::RackBuy {
                outfit_id: 1,
                wear_now: true
            })
            .unwrap(),
        AuthoringWire::Eod {
            incarnation: 44,
            plugin_id: RACK_CUSTOMER_PLUGIN,
            event_name: "rack_purchase".into(),
            text: "1,true".into()
        }
    );
}
#[test]
fn original_admin_move_delete_bypass_does_not_apply_to_inventory_return() {
    let mut snapshot = snapshot();
    snapshot.permission = Some(4);
    snapshot.objects[0].movable = Some(false);
    snapshot.objects[0].transaction_incomplete = true;
    let entity = snapshot.objects[0].entity.clone();
    assert!(
        snapshot
            .prepare(&AuthoringIntent::Move {
                entity: entity.clone(),
                placement: Placement {
                    x: 16,
                    y: 32,
                    level: 1,
                    direction: 1
                }
            })
            .is_ok()
    );
    assert!(
        snapshot
            .prepare(&AuthoringIntent::Delete {
                entity: entity.clone(),
                desired_mode: 2
            })
            .is_ok()
    );
    assert!(
        snapshot
            .prepare(&AuthoringIntent::SendToInventory { entity })
            .is_err()
    );
}
