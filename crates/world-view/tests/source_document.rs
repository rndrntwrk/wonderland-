use wonderland_world_view::*;

// Catches silently substituting VisualLot::flat/synthetic_lot, losing source rows,
// treating XML level 0 as off-world architecture, or dropping controller records.
#[test]
fn original_blueprint_preserves_source_dimensions_records_and_off_world_objects() {
    let document = WorldDocument::original_empty_lot().unwrap();
    assert_eq!(
        (document.lot.width, document.lot.height, document.lot.levels),
        (77, 77, 5)
    );
    assert_eq!(
        document.source_counts,
        Some(SourceCounts {
            floors: 1269,
            walls: 0,
            pools: 0,
            objects: 18
        })
    );
    assert_eq!(
        document
            .lot
            .tiles
            .iter()
            .filter(|tile| tile.floor != 0)
            .count(),
        1269
    );
    assert_eq!(document.lot.tiles[78].floor, 9);
    assert_eq!(document.lot.tiles[77 * 4 + 1].floor, 10);
    assert_eq!(document.objects.len(), 18);
    assert_eq!(
        document
            .objects
            .iter()
            .filter(|object| object.level == 0)
            .count(),
        10
    );
    assert_eq!(
        document
            .objects
            .iter()
            .filter(|object| object.visible)
            .count(),
        8
    );
    assert!(
        document
            .objects
            .iter()
            .all(|object| object.entity.is_none() && object.model.is_none())
    );
    let phone = &document.objects[8];
    assert_eq!(phone.source_guid, 0x313D2F9A);
    assert_eq!(
        (phone.position_tiles.x, phone.position_tiles.y, phone.level),
        (46., 71., 1)
    );
    assert_eq!(document.objects[16].blueprint.unwrap().direction, i32::MIN);
    assert_eq!(document.provenance.kind, WorldSourceKind::OriginalXml);
    assert_eq!(document.revision.lot_id, None);
    assert!(
        document
            .diagnostics
            .iter()
            .any(|entry| entry.code == "missing_object_model")
    );
    document.validate().unwrap();
}

// Catches accepting out-of-bounds architecture, numeric wrap, ambiguous duplicate
// tiles or an external entity/DTD through the public source XML boundary.
#[test]
fn malformed_source_architecture_is_rejected_before_mesh_allocation() {
    for body in [
        "<floor level=\"0\" x=\"77\" y=\"1\" value=\"9\"/>",
        "<floor level=\"0\" x=\"-1\" y=\"1\" value=\"9\"/>",
        "<floor level=\"-1\" x=\"1\" y=\"1\" value=\"9\"/>",
        "<floor level=\"0\" x=\"1\" y=\"1\" value=\"65536\"/>",
        "<floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/>",
    ] {
        let xml = format!("<house><size>77</size><world><floors>{body}</floors></world></house>");
        assert!(
            WorldDocument::from_blueprint_xml(&xml, "fixture", "test").is_err(),
            "accepted {body}"
        );
    }
    assert!(WorldDocument::from_blueprint_xml("<!DOCTYPE house [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><house><size>&x;</size></house>", "fixture", "test").is_err());
}

// Catches raw terrain reshape, clamping or modulo at a TerrainComponent zero
// boundary. Centers deliberately follow VMArchitectureTerrain's modulo average.
#[test]
fn nonflat_source_terrain_uses_original_right_and_bottom_zero_edges() {
    let terrain = source_terrain(2, 2, &[10, 20, 30, 40], 5).unwrap();
    assert_eq!(terrain.corners, vec![10, 20, 0, 30, 40, 0, 0, 0, 0]);
    assert_eq!(terrain.altitude_centers, vec![25, 25, 25, 25]);
    assert_eq!(terrain.base_alt, 5);
    assert_eq!(terrain.boundary, TerrainBoundary::TerrainComponentZero);
    assert!(source_terrain(2, 2, &[10, 20, 30], 0).is_err());
}

// Catches schema/array count, nonfinite data, or stale duplicate instance IDs
// being admitted to rendering from a normalized live document.
#[test]
fn normalized_world_validates_lengths_coordinates_and_identity() {
    let original = WorldDocument::original_empty_lot().unwrap();
    let mut invalid = original.clone();
    invalid.lot.terrain.corners.pop();
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.schema_version += 1;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.lot.tiles.pop();
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.objects[8].position_tiles.x = f32::NAN;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.objects[8].position_tiles.x = 77.;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.source_counts.as_mut().unwrap().objects = 100;
    assert!(invalid.validate().is_err());
}

// Catches parsing live 64-bit identities or dynamic part masks through a JSON
// number, which rounds valid legacy identities in JavaScript.
#[test]
fn normalized_world_json_keeps_u64_identities_as_decimal_strings() {
    let mut document = WorldDocument::original_empty_lot().unwrap();
    document.revision.lot_id = Some(18_446_744_073_709_551_614);
    document.revision.epoch = 9_007_199_254_740_993;
    document.objects[8].dynamic_flags = [u64::MAX, 9_007_199_254_740_993];
    let encoded = serde_json::to_value(&document).unwrap();
    assert_eq!(encoded["revision"]["lot_id"], "18446744073709551614");
    assert_eq!(encoded["revision"]["epoch"], "9007199254740993");
    assert_eq!(
        encoded["objects"][8]["dynamic_flags"][0],
        "18446744073709551615"
    );
    let roundtrip: WorldDocument = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(roundtrip.revision, document.revision);
    assert_eq!(
        roundtrip.objects[8].dynamic_flags,
        document.objects[8].dynamic_flags
    );
    let mut invalid = encoded;
    invalid["revision"]["epoch"] = serde_json::json!(9007199254740993u64);
    assert!(serde_json::from_value::<WorldDocument>(invalid).is_err());
}
