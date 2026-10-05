use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, Need};

fn fixture() -> AuthoringProjection {
    serde_json::from_str(include_str!("../../../fixtures/ui/authoring-v1.json")).unwrap()
}

#[test]
fn authoring_fixture_round_trips_five_isolated_empty_homes_and_six_catalog_records() {
    let projection = fixture();
    projection.validate().unwrap();
    let expected = [
        ("maya", "Maya", VisualIdentity::Maya),
        ("jules", "Jules", VisualIdentity::Jules),
        ("nico", "Nico", VisualIdentity::Nico),
        ("amara", "Amara", VisualIdentity::Amara),
        ("leo", "Leo", VisualIdentity::Leo),
    ];
    assert_eq!(projection.profiles.len(), 5);
    for (profile, (id, name, identity)) in projection.profiles.iter().zip(expected) {
        assert_eq!(profile.character.id.as_ref(), id);
        assert_eq!(profile.character.name, name);
        assert_eq!(profile.identity, identity);
        assert_eq!(profile.look_id.as_ref(), format!("{id}-everyday"));
        assert_eq!(profile.character.money, 1_250);
        assert_eq!(profile.home.owner_id, profile.character.id);
        assert!(profile.home.instances.is_empty());
        assert!(profile.home.permissions.purchase.is_available());
        assert!(profile.home.permissions.arrange.is_available());
    }
    let records = projection
        .catalog
        .iter()
        .map(|item| {
            (
                item.id.as_ref(),
                item.name.as_str(),
                item.category,
                item.price,
                item.footprint.width,
                item.footprint.depth,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        records,
        [
            (
                "armchair",
                "Harbor armchair",
                CatalogCategory::Living,
                180,
                1,
                1
            ),
            (
                "coffee-table",
                "Oak coffee table",
                CatalogCategory::Living,
                120,
                2,
                1
            ),
            (
                "floor-lamp",
                "Linen floor lamp",
                CatalogCategory::Lighting,
                90,
                1,
                1
            ),
            ("fern", "Potted fern", CatalogCategory::Decor, 45, 1, 1),
            (
                "bookcase",
                "Oak bookcase",
                CatalogCategory::Storage,
                260,
                2,
                1
            ),
            ("woven-rug", "Woven rug", CatalogCategory::Decor, 160, 2, 2),
        ]
    );
    let encoded = serde_json::to_vec(&projection).unwrap();
    assert!(encoded.len() < MAX_AUTHORING_JSON_BYTES);
    let decoded: AuthoringProjection = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, projection);
}

#[test]
fn every_visual_identity_accepts_its_three_looks_and_rejects_cross_identity_looks() {
    for identity in VisualIdentity::ALL {
        for style in LookStyle::ALL {
            let mut projection = fixture();
            projection.profiles[0].identity = identity;
            projection.profiles[0].look_id = identity.look_id(style);
            projection.validate().unwrap();
            assert_eq!(
                projection.profiles[0].look_id.style_for(identity),
                Some(style)
            );
            let other = if identity == VisualIdentity::Maya {
                VisualIdentity::Jules
            } else {
                VisualIdentity::Maya
            };
            projection.profiles[0].look_id = other.look_id(style);
            assert!(projection.validate().is_err());
        }
    }
    let mut projection = fixture();
    projection.profiles[0].look_id = "maya-invented".into();
    assert!(projection.validate().is_err());
}

#[test]
fn names_trim_ordinary_unicode_but_enforce_scalar_byte_and_control_bounds() {
    assert_eq!(
        normalize_profile_name("  Zoë 李 🐚  ").unwrap(),
        "Zoë 李 🐚"
    );
    assert_eq!(
        normalize_profile_name(&"🪷".repeat(32)).unwrap(),
        "🪷".repeat(32)
    );
    assert_eq!(normalize_profile_name("e\u{301}").unwrap(), "e\u{301}");
    for name in ["", " \u{2003} ", "a\nb", "a\0b", "\nMaya", "Maya\t"] {
        assert!(matches!(
            normalize_profile_name(name),
            Err(AuthoringError::InvalidName(_))
        ));
    }
    assert!(normalize_profile_name(&"é".repeat(33)).is_err());
    assert!(normalize_profile_name(&"🪷".repeat(33)).is_err());
    let mut projection = fixture();
    projection.profiles[0].character.name = " Maya ".into();
    assert!(
        projection.validate().is_err(),
        "persisted names must already be trimmed"
    );
}

#[test]
fn snapshot_validation_rejects_invalid_profile_identity_permissions_budget_and_needs() {
    let original = fixture();
    let mut cases = Vec::new();
    let mut p = original.clone();
    p.version = 2;
    cases.push(p);
    let mut p = original.clone();
    p.revision = 0;
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.id = "bad id".into();
    cases.push(p);
    let mut p = original.clone();
    p.profiles.push(p.profiles[0].clone());
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].home.owner_id = "jules".into();
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.money = -1;
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.money = 1_251;
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.needs.remove(&Need::Room);
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.needs.insert(Need::Energy, 101);
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].home.permissions.purchase = Availability::Unavailable {
        reason: String::new(),
    };
    cases.push(p);
    let mut p = original;
    for index in 0..4 {
        let mut profile = p.profiles[0].clone();
        profile.character.id = format!("extra-{index}").into();
        profile.home.owner_id = profile.character.id.clone();
        p.profiles.push(profile);
    }
    cases.push(p);
    for p in cases {
        assert!(p.validate().is_err());
    }
}

#[test]
fn authored_catalog_rejects_changed_prices_shapes_categories_and_unknown_records() {
    let original = fixture();
    let mut p = original.clone();
    p.catalog[0].price = 1;
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.catalog[0].footprint.width = 8;
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.catalog[0].category = CatalogCategory::Decor;
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.catalog[0].id = "unknown".into();
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.catalog.push(p.catalog[0].clone());
    assert!(p.validate().is_err());
    let mut p = original;
    p.catalog.clear();
    p.validate().unwrap();
}

#[test]
fn snapshot_rejects_duplicate_unknown_overlapping_entry_and_outside_instances() {
    let mut original = fixture();
    original.profiles[0].home.instances.push(OwnedInstance {
        id: "chair-1".into(),
        catalog_id: "armchair".into(),
        placement: Some(GridPose {
            cell: GridCell { x: 1, y: 1 },
            direction: Direction::North,
        }),
    });
    original.validate().unwrap();
    let mut p = original.clone();
    p.profiles[1].home.instances = p.profiles[0].home.instances.clone();
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.profiles[0].home.instances[0].catalog_id = "unknown".into();
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.profiles[0].home.instances[0].id = "bad id".into();
    assert!(p.validate().is_err());
    for cell in [
        GridCell { x: 0, y: 0 },
        GridCell { x: -1, y: 3 },
        GridCell { x: 8, y: 1 },
        GridCell { x: 1, y: 6 },
    ] {
        let mut p = original.clone();
        p.profiles[0].home.instances[0]
            .placement
            .as_mut()
            .unwrap()
            .cell = cell;
        assert!(p.validate().is_err());
    }
    let mut p = original.clone();
    let mut duplicate_cell = p.profiles[0].home.instances[0].clone();
    duplicate_cell.id = "chair-2".into();
    p.profiles[0].home.instances.push(duplicate_cell);
    assert!(p.validate().is_err());
    let mut p = original;
    p.profiles[0].home.instances = (0..65)
        .map(|i| OwnedInstance {
            id: format!("stored-{i}").into(),
            catalog_id: "fern".into(),
            placement: None,
        })
        .collect();
    assert!(p.validate().is_err());
}

#[test]
fn rotated_footprints_use_integer_cells_and_reserve_the_entrance() {
    let footprint = Footprint { width: 2, depth: 1 };
    assert_eq!(
        footprint.rotated(Direction::North),
        Footprint { width: 2, depth: 1 }
    );
    assert_eq!(
        footprint.rotated(Direction::East),
        Footprint { width: 1, depth: 2 }
    );
    assert_eq!(
        footprint.rotated(Direction::South),
        Footprint { width: 2, depth: 1 }
    );
    assert_eq!(
        footprint.rotated(Direction::West),
        Footprint { width: 1, depth: 2 }
    );
    assert_eq!(
        footprint
            .cells(GridPose {
                cell: GridCell { x: 2, y: 3 },
                direction: Direction::East
            })
            .unwrap(),
        [GridCell { x: 2, y: 3 }, GridCell { x: 2, y: 4 }]
    );
    assert_eq!(
        footprint.cells(GridPose {
            cell: GridCell { x: 7, y: 5 },
            direction: Direction::North
        }),
        Err(AuthoringError::OutOfBounds)
    );
    assert_eq!(
        footprint.cells(GridPose {
            cell: GridCell { x: 0, y: 0 },
            direction: Direction::North
        }),
        Err(AuthoringError::EntranceReserved)
    );
    assert_eq!(
        Direction::North
            .clockwise()
            .clockwise()
            .clockwise()
            .clockwise(),
        Direction::North
    );
}
