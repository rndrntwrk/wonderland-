use wonderland_vm_protocol::snapshot::*;
use wonderland_web_shell::snapshot_world::snapshot_world;
use wonderland_world_view::*;

// Clearly identified fixture for the conversion boundary. Wire parsing itself
// is exercised by vm-protocol's complete FSOv golden serialization fixtures.
#[path = "support/snapshot.rs"]
mod support;
use support::fixture;

// Catches source packed location being mislabeled as a database lot ID, or a
// snapshot's reusable short ObjectID being forged into a live EntityRef.
#[test]
fn snapshot_identity_stays_distinct_from_server_authority_and_retains_exact_source_ids() {
    let source = fixture();
    let world = snapshot_world(&source, 9, 123, 27).unwrap();
    assert_eq!(world.provenance.kind, WorldSourceKind::LegacySnapshot);
    assert!(world.provenance.origin.contains("012301AB"));
    assert_eq!(world.revision.lot_id, None);
    assert_eq!(
        (
            world.revision.epoch,
            world.revision.tick,
            world.revision.architecture_revision
        ),
        (9, 123, 27)
    );
    let object = &world.objects[0];
    assert!(object.entity.is_none() && object.blueprint.is_none());
    assert_eq!(object.source_guid, 0xFEDCBA98);
    let identity = object.snapshot.unwrap();
    assert_eq!(
        (identity.object_id, identity.persistent_id, identity.record),
        (7, 4_000_000_001, 0)
    );
    assert_eq!((identity.x, identity.y, identity.level), (24, 24, 2));
    assert_eq!(identity.presentation_generation, 27);
    assert_eq!(object.dynamic_flags, [u64::MAX, 9_007_199_254_740_993]);
    assert!(
        world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "snapshot_presentation_revision")
    );
}

// Catches swapped source wall patterns, lost diagonal floors, fabricated room
// state, wrong grass colors, or reinterpreting 1/16-tile positions as big tiles.
#[test]
fn snapshot_geometry_and_visual_pose_follow_original_source_conversions() {
    let world = snapshot_world(&fixture(), 9, 123, 27).unwrap();
    assert_eq!(
        (world.lot.width, world.lot.height, world.lot.levels),
        (3, 3, 2)
    );
    assert_eq!(world.lot.tiles[4].floor, 9);
    assert_eq!(world.lot.tiles[4].wall.patterns, [9, 10, 11, 12]);
    assert!(
        world.lot.tiles[4].wall.west
            && world.lot.tiles[4].wall.north
            && world.lot.tiles[4].wall.south
            && world.lot.tiles[4].wall.east
    );
    assert_eq!(world.lot.tiles[13].half_floors, Some([13, 17]));
    assert!(
        world
            .lot
            .tiles
            .iter()
            .all(|tile| tile.indoors.is_none() && tile.supported.is_none())
    );
    assert_eq!(
        world.lot.terrain.corners,
        vec![0, 0, 0, 0, 0, 16, 32, 0, 0, 48, 64, 0, 0, 0, 0, 0]
    );
    assert_eq!(world.lot.terrain.grass, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(
        world.lot.terrain.light,
        [80. / 255., 116. / 255., 59. / 255., 1.]
    );
    assert_eq!(
        world.lot.terrain.dark,
        [181. / 255., 171. / 255., 149. / 255., 1.]
    );
    let object = &world.objects[0];
    // VMGameObject subtracts half a tile before the renderer's +1.5 graphics
    // center offset. Bilinear height at (1.5,1.5) is 40 raw units =0.75 tiles.
    assert_eq!((object.position_tiles.x, object.position_tiles.y), (1., 1.));
    assert!((object.position_tiles.z - 3.70).abs() < 0.00001);
    assert!((object.yaw_radians - std::f32::consts::FRAC_PI_2).abs() < 0.00001);
    assert!(object.model.is_none());
    assert_eq!(world.lot.roof.unwrap().material, 16);
    world.validate().unwrap();
}

// Catches the original OUT_OF_WORLD sentinel being rendered as an in-lot prop
// merely because the serialized level is1, and rejects malformed DTO shapes.
#[test]
fn snapshot_off_world_objects_and_invalid_geometry_are_handled_without_fake_placements() {
    let mut source = fixture();
    source.entities[0].position = Position {
        x: i16::MIN,
        y: i16::MIN,
        level: 1,
    };
    let world = snapshot_world(&source, 9, 123, 27).unwrap();
    assert!(!world.objects[0].visible && !world.objects[0].selectable);
    assert_eq!(world.objects[0].level, 0);
    assert_eq!(world.objects[0].snapshot.unwrap().level, 1);
    source.context.architecture.floors[0].pop();
    assert!(snapshot_world(&source, 9, 123, 27).is_err());
    let mut source = fixture();
    source.context.architecture.terrain_light = 255;
    assert!(snapshot_world(&source, 9, 123, 27).is_err());
}

// A valid source center at zero becomes a -.5 WorldUI corner coordinate, and
// object contact must clamp to source interior samples instead of ground zero.
#[test]
fn snapshot_boundary_centers_visibility_and_directions_match_source() {
    let mut source = fixture();
    source.entities[0].position = Position {
        x: 0,
        y: 0,
        level: 1,
    };
    let world = snapshot_world(&source, 9, 123, 27).unwrap();
    let object = &world.objects[0];
    assert_eq!(
        (object.position_tiles.x, object.position_tiles.y),
        (-0.5, -0.5)
    );
    assert!((object.position_tiles.z - 0.3).abs() < 0.00001);
    source.entities[0].object_data = vec![0; 80];
    source.entities[0].object_data[34] = 1;
    source.entities[0].object_data[29] = 7;
    let world = snapshot_world(&source, 9, 123, 28).unwrap();
    assert!(!world.objects[0].visible && !world.objects[0].selectable);
    assert_eq!(world.objects[0].room, 8);
    source.entities[0].appearance = Appearance::Object {
        direction: 3,
        disabled: 0,
    };
    let world = snapshot_world(&source, 9, 123, 29).unwrap();
    assert!((world.objects[0].yaw_radians - std::f32::consts::FRAC_PI_4).abs() < 0.00001);
    assert!(
        world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "source_noncanonical_direction")
    );
    source.entities[0].appearance = Appearance::Object {
        direction: 0,
        disabled: 0,
    };
    assert_eq!(
        snapshot_world(&source, 9, 123, 30).unwrap().objects[0].yaw_radians,
        0.
    );
}

// Pool is a source floor pattern on a level; its count is not limited to one
// ground-plane area when a complete snapshot carries several stories.
#[test]
fn snapshot_retains_pool_floor_patterns_on_each_serialized_level() {
    let mut source = fixture();
    source.context.architecture.floors = vec![vec![65_535; 9]; 2];
    let world = snapshot_world(&source, 9, 123, 27).unwrap();
    assert_eq!(world.source_counts.unwrap().pools, 18);
    assert!(world.lot.tiles.iter().all(|tile| tile.floor == 65_535));
}
