use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, Need};

fn fixture() -> AuthoringProjection {
    serde_json::from_str(include_str!("../../../fixtures/ui/authoring-v2.json")).unwrap()
}

fn pose(x: i16, y: i16, level: i16, direction: Direction) -> GridPose {
    GridPose {
        cell: GridCell { x, y },
        level,
        direction,
    }
}

#[test]
fn fixture_round_trip_keeps_isolated_homes_and_illustrations_without_game_outfit_ids() {
    let projection = fixture();
    projection.validate().unwrap();
    for profile in &projection.profiles {
        assert_eq!(profile.home.owner_id, profile.character.id);
        assert!(profile.home.instances.is_empty());
        assert!(profile.appearance.head.is_none());
        assert!(profile.appearance.body.is_none());
        assert!(profile.wardrobe.outfits.is_empty());
        assert!(profile.portrait.is_some());
    }
    let encoded = serde_json::to_vec(&projection).unwrap();
    assert!(encoded.len() < MAX_AUTHORING_JSON_BYTES);
    assert_eq!(
        serde_json::from_slice::<AuthoringProjection>(&encoded).unwrap(),
        projection
    );
}

#[test]
fn names_preserve_unicode_while_account_field_rules_apply_only_to_new_requests() {
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
    assert!(normalize_profile_name(&"🪷".repeat(33)).is_err());
    let mut projection = fixture();
    projection.profiles[0].character.name = "Zoë 李".into();
    projection.account.fields.name_alphabet = NameAlphabet::AsciiLettersAndSpaces;
    projection.account.fields.minimum_name_characters = 3;
    projection.account.fields.maximum_name_characters = 24;
    projection.validate().unwrap();
    assert!(
        projection
            .account
            .fields
            .validate_fields("Zoë 李", "")
            .is_err()
    );
    assert!(
        projection
            .account
            .fields
            .validate_fields("Maya", &"x".repeat(500))
            .is_err()
    );
    projection
        .account
        .fields
        .validate_fields("Maya", &"x".repeat(499))
        .unwrap();
}

#[test]
fn structural_validation_rejects_bad_ids_duplicates_permissions_negative_money_and_needs() {
    let original = fixture();
    let mut cases = Vec::new();
    let mut p = original.clone();
    p.version = AUTHORING_VERSION + 1;
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
    p.profiles[0].character.needs.remove(&Need::Room);
    cases.push(p);
    let mut p = original.clone();
    p.profiles[0].character.needs.insert(Need::Energy, 101);
    cases.push(p);
    let mut p = original;
    p.profiles[0].home.permissions.purchase = Availability::Unavailable {
        reason: String::new(),
    };
    cases.push(p);
    for projection in cases {
        assert!(projection.validate().is_err());
    }
}

#[test]
fn arbitrary_supplied_categories_rotation_and_prices_are_validated_by_structure() {
    let mut p = fixture();
    p.catalog_categories.push(CatalogCategory {
        id: "source-musical".into(),
        label: "Musical instruments".into(),
    });
    p.catalog[0].category = "source-musical".into();
    p.catalog[0].id = "source-piano".into();
    p.catalog[0].source_key = "00000000A0010000:00000001".into();
    p.catalog[0].price = 4500;
    p.catalog[0].footprint = Footprint { width: 3, depth: 2 };
    p.catalog[0].rotations = vec![Direction::East, Direction::West];
    p.validate().unwrap();
    assert_eq!(
        p.catalog[0].next_direction(Direction::East),
        Some(Direction::West)
    );
    assert!(p.catalog[0].can_rotate());
    let good = p.clone();
    p.catalog[0].price = -1;
    assert!(p.validate().is_err());
    let mut p = good.clone();
    p.catalog[0].category = "missing".into();
    assert!(p.validate().is_err());
    let mut p = good.clone();
    p.catalog[0].rotations.push(Direction::East);
    assert!(p.validate().is_err());
    let mut p = good;
    p.catalog.push(p.catalog[0].clone());
    assert!(p.validate().is_err());
}

#[test]
fn placement_validation_uses_supplied_bounds_reserved_cells_and_distinct_levels() {
    let mut original = fixture();
    original.profiles[0].home.lot.levels = vec![0, 1];
    original.profiles[0].home.instances = vec![
        OwnedInstance {
            id: "chair-1".into(),
            catalog_id: "armchair".into(),
            owner_id: Some("maya".into()),
            placement: Some(pose(1, 1, 0, Direction::North)),
        },
        OwnedInstance {
            id: "chair-2".into(),
            catalog_id: "armchair".into(),
            owner_id: Some("maya".into()),
            placement: Some(pose(1, 1, 1, Direction::North)),
        },
    ];
    original.validate().unwrap();
    let mut p = original.clone();
    p.profiles[0].home.instances[1].placement = Some(pose(1, 1, 0, Direction::North));
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.profiles[1].home.instances = p.profiles[0].home.instances.clone();
    assert!(p.validate().is_err());
    let mut p = original.clone();
    p.profiles[0].home.instances[0].catalog_id = "missing".into();
    assert!(p.validate().is_err());
    for candidate in [
        pose(0, 0, 0, Direction::North),
        pose(-1, 3, 0, Direction::North),
        pose(8, 1, 0, Direction::North),
        pose(1, 6, 0, Direction::North),
        pose(1, 1, 7, Direction::North),
    ] {
        let mut p = original.clone();
        p.profiles[0].home.instances[0].placement = Some(candidate);
        assert!(p.validate().is_err());
    }
}

#[test]
fn rotated_footprints_return_level_aware_cells_without_a_global_room_size() {
    let footprint = Footprint { width: 2, depth: 1 };
    let mut lot = fixture().profiles.remove(0).home.lot;
    assert_eq!(
        footprint.rotated(Direction::East),
        Footprint { width: 1, depth: 2 }
    );
    assert_eq!(
        footprint
            .cells(pose(2, 3, 0, Direction::East), &lot)
            .unwrap(),
        [
            LotCell {
                cell: GridCell { x: 2, y: 3 },
                level: 0
            },
            LotCell {
                cell: GridCell { x: 2, y: 4 },
                level: 0
            }
        ]
    );
    assert_eq!(
        footprint.cells(pose(7, 5, 0, Direction::North), &lot),
        Err(AuthoringError::OutOfBounds)
    );
    lot.bounds.width = 64;
    lot.bounds.depth = 48;
    lot.levels.push(3);
    assert_eq!(
        footprint
            .cells(pose(31, 42, 3, Direction::North), &lot)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        footprint.cells(pose(0, 0, 0, Direction::North), &lot),
        Err(AuthoringError::EntranceReserved)
    );
}
