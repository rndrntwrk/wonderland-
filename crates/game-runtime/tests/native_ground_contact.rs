//! Native terrain uses explicit boundary vertices; no routing or VM coordinates change.
use sim_core::{
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{EntityField, MemoryAddress, RoutineStore},
};
use wonderland_game_runtime::*;
use wonderland_render_core::Vec3;
use wonderland_world_view::{WorldDocument, WorldTile, source_terrain};

fn setup(
    width: u16,
    height: u16,
    level: u8,
    avatar: bool,
    position: LotPosition,
    base_alt: i16,
    elevation: impl Fn(u16, u16) -> i16,
) -> (GameRuntime, WorldDocument, EntityRef) {
    let mut appearance = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:native-ground-contact",
        "authored geometry, no original art",
    )
    .unwrap();
    appearance.lot.width = width;
    appearance.lot.height = height;
    appearance.lot.levels = level;
    appearance.lot.tiles =
        vec![WorldTile::default(); usize::from(width) * usize::from(height) * usize::from(level)];
    appearance.lot.terrain = source_terrain(
        width,
        height,
        &vec![0; usize::from(width) * usize::from(height)],
        base_alt,
    )
    .unwrap();
    appearance.source_counts = None;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(123, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut lot = LotModel::new(width, height, level).unwrap();
    for y in 0..=height {
        for x in 0..=width {
            lot.set_terrain_vertex(x, y, elevation(x, y)).unwrap();
        }
    }
    let mut game = GameRuntime::new(
        content,
        lot,
        RuntimeConfig::new(VmMode::Ts1, 77, 9, 17),
        RuntimeRole::Authority,
    )
    .unwrap();
    let out = game
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position,
            facing: Facing::EAST,
            persistent_id: PersistentId(42),
            avatar,
        })])
        .unwrap();
    let entity = out
        .events
        .iter()
        .find_map(|e| match e {
            RuntimeEvent::Spawned(e) => Some(*e),
            _ => None,
        })
        .unwrap();
    (game, appearance, entity)
}
fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.00002,
        "actual {actual}, expected {expected}"
    );
}
#[test]
fn avatars_sample_accepted_terrain_at_subtile_coordinates_without_mutating_the_vm() {
    let p = LotPosition::new(52, 76, 1); // (3.25, 4.75), not a rounded tile center.
    let (game, appearance, entity) = setup(8, 8, 1, true, p, 8, |x, y| (8 * x + 16 * y) as i16);
    let before = game.snapshot().unwrap();
    let world = game.world_document(&appearance).unwrap();
    let o = &world.objects[0];
    close(o.position_tiles.x, 3.25);
    close(o.position_tiles.y, 4.75);
    close(o.position_tiles.z, (102. - 8.) * 3. / 160.);
    assert_eq!(o.entity.unwrap().generation, entity.generation);
    assert_eq!(game.projection().entities[0].position, p);
    assert_eq!(game.snapshot().unwrap(), before);
}
#[test]
fn object_mesh_offset_does_not_shift_the_terrain_sampling_point() {
    let (game, appearance, _) = setup(8, 8, 1, false, LotPosition::new(52, 76, 1), 8, |x, y| {
        (8 * x + 16 * y) as i16
    });
    let o = game.world_document(&appearance).unwrap().objects.remove(0);
    close(o.position_tiles.x, 2.75);
    close(o.position_tiles.y, 4.25);
    close(o.position_tiles.z, (102. - 8.) * 3. / 160.);
}
#[test]
fn upper_floor_contact_adds_story_height_to_the_same_ground_elevation() {
    let (game, appearance, _) = setup(8, 8, 3, true, LotPosition::new(52, 76, 3), 8, |x, y| {
        (8 * x + 16 * y) as i16
    });
    let world = game.world_document(&appearance).unwrap();
    close(
        world.objects[0].position_tiles.z,
        (102. - 8.) * 3. / 160. + 2. * 2.95,
    );
    assert_eq!(world.objects[0].level, 3);
}
#[test]
fn nonplanar_terrain_interpolates_all_four_accepted_corners() {
    let (game, appearance, _) = setup(
        8,
        8,
        1,
        true,
        LotPosition::new(52, 76, 1),
        10,
        |x, y| match (x, y) {
            (3, 4) => 0,
            (4, 4) => 160,
            (3, 5) => 320,
            (4, 5) => 640,
            _ => 0,
        },
    );
    close(
        game.world_document(&appearance).unwrap().objects[0]
            .position_tiles
            .z,
        5.625,
    );
}
#[test]
fn rectangular_boundary_centers_and_contact_use_explicit_edges_not_wrapped_neighbors() {
    let (game, appearance, _) = setup(5, 7, 1, true, LotPosition::new(76, 108, 1), 0, |x, y| {
        (x * 10 + y * 100) as i16
    });
    let world = game.world_document(&appearance).unwrap();
    assert_eq!(world.lot.terrain.altitude_centers[6 * 5 + 4], 695);
    close(world.objects[0].position_tiles.z, 722.5 * 3. / 160.);
    assert_eq!(world.lot.terrain.corners[7 * 6 + 5], 750);
}
#[test]
fn negative_center_averaging_uses_wide_sum_and_truncates_towards_zero() {
    let (game, appearance, _) = setup(8, 8, 1, false, LotPosition::new(56, 56, 1), -20, |x, y| {
        if (x, y) == (7, 7) {
            -7
        } else if x == 8 || y == 8 {
            -10
        } else {
            0
        }
    });
    let world = game.world_document(&appearance).unwrap();
    assert_eq!(world.lot.terrain.altitude_centers[63], -9); // -37 / 4, not wrapped -7/4
    close(world.objects[0].position_tiles.z, 20. * 3. / 160.);
}
#[test]
fn every_nonzero_original_hidden_value_suppresses_visibility_and_selection() {
    let (mut game, appearance, actor) =
        setup(8, 8, 1, true, LotPosition::new(56, 56, 1), 0, |_, _| 16);
    for hidden in [1, 2, -1, i16::MAX, 0] {
        game.advance(vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::ObjectData,
                index: 34,
            },
            value: hidden,
        }])
        .unwrap();
        let o = game.world_document(&appearance).unwrap().objects.remove(0);
        assert_eq!(o.visible, hidden == 0, "Hidden={hidden}");
        assert_eq!(o.selectable, hidden == 0, "Hidden={hidden}");
    }
}
#[test]
fn out_of_world_entities_never_sample_terrain_or_gain_pick_targets() {
    let (game, appearance, _) = setup(8, 8, 1, true, LotPosition::OUT_OF_WORLD, 0, |_, _| i16::MAX);
    let o = game.world_document(&appearance).unwrap().objects.remove(0);
    assert_eq!(o.position_tiles, Vec3::ZERO);
    assert_eq!(o.level, 0);
    assert!(!o.visible && !o.selectable);
}
#[test]
fn snapshot_recovery_preserves_the_exact_contact_without_double_applying_elevation() {
    let (game, appearance, _) = setup(8, 8, 2, true, LotPosition::new(52, 76, 2), 8, |x, y| {
        (8 * x + 16 * y) as i16
    });
    let mut restored = GameRuntime::new(
        game.sim().content().clone(),
        LotModel::new(8, 8, 2).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 77, 9, 17),
        RuntimeRole::Replica,
    )
    .unwrap();
    restored.restore(&game.snapshot().unwrap()).unwrap();
    let a = game.world_document(&appearance).unwrap();
    let b = restored.world_document(&a).unwrap();
    assert_eq!(a, b);
    close(
        b.objects[0].position_tiles.z,
        (102. - 8.) * 3. / 160. + 2.95,
    );
}
