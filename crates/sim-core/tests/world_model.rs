use sim_core::{
    ids::{EntityRef, ObjectId},
    world::*,
};
use std::collections::BTreeSet;

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}

#[test]
fn diagonal_room_words_match_source_half_tests_and_survive_roundtrip() {
    let mut lot = LotModel::new(6, 6, 2).unwrap();
    let tile = TilePos::new(2, 2, 1);
    let mut wall = WallTile::solid(15);
    wall.diagonal = Diagonal::Vertical;
    wall.diagonal_solid = true;
    wall.half_floors = Some([7, 9]);
    lot.set_wall(tile, wall.clone()).unwrap();
    let low = LotPosition::new(44, 36, 1);
    let high = LotPosition::new(36, 44, 1);
    assert_ne!(lot.room_at(low), lot.room_at(high));
    assert_eq!(lot.floor_at(low), Some(7));
    assert_eq!(lot.floor_at(high), Some(9));
    assert!(!lot.rooms().room(lot.room_at(low).unwrap()).unwrap().outside);
    assert!(
        !lot.rooms()
            .room(lot.room_at(high).unwrap())
            .unwrap()
            .outside
    );
    assert_ne!(
        lot.room_at(low),
        lot.room_at(LotPosition { level: 2, ..low })
    );
    wall.diagonal = Diagonal::Horizontal;
    lot.set_wall(tile, wall).unwrap();
    assert_eq!(lot.floor_at(LotPosition::new(39, 40, 1)), Some(7));
    assert_eq!(lot.floor_at(LotPosition::new(40, 40, 1)), Some(9));
    let bytes = bincode::serialize(&lot).unwrap();
    let restored: LotModel = bincode::deserialize(&bytes).unwrap();
    assert_eq!(restored, lot);
    restored.validate().unwrap();
}

#[test]
fn wall_fences_partition_rooms_and_transmit_outside_status() {
    let mut lot = LotModel::new(5, 5, 1).unwrap();
    let tile = TilePos::new(2, 2, 1);
    lot.set_wall(tile, WallTile::solid(15)).unwrap();
    let room = lot.room_at(tile.center()).unwrap();
    assert!(!lot.rooms().room(room).unwrap().outside);
    let mut fence = WallTile::solid(15);
    fence.room_separators &= !Cardinal::North.wall_bit();
    lot.set_wall(tile, fence).unwrap();
    let room = lot.room_at(tile.center()).unwrap();
    assert!(lot.rooms().room(room).unwrap().outside);
    assert!(!lot.rooms().room(room).unwrap().adjacent.is_empty());
    assert!(lot.edge_blocked(tile, Cardinal::North));
    assert!(!lot.edge_separates_room(tile, Cardinal::North));
}

#[test]
fn terrain_and_unrelated_floor_changes_have_precise_dirty_sets() {
    let mut lot = LotModel::new(8, 8, 2).unwrap();
    let revision = lot.revision();
    lot.set_terrain_vertex(3, 4, 25).unwrap();
    let expected = BTreeSet::from([
        TilePos::new(2, 3, 1),
        TilePos::new(3, 3, 1),
        TilePos::new(2, 4, 1),
        TilePos::new(3, 4, 1),
    ]);
    assert_eq!(lot.dirty().terrain, expected);
    assert_eq!(lot.dirty().routing, expected);
    assert!(lot.dirty().rooms.is_empty());
    assert_eq!(lot.revision().rooms, revision.rooms);
    let terrain_revision = lot.revision();
    assert!(!lot.set_terrain_vertex(3, 4, 25).unwrap());
    assert_eq!(lot.revision(), terrain_revision);
    lot.take_dirty();
    lot.set_floor(TilePos::new(1, 1, 1), 10).unwrap();
    lot.set_floor(TilePos::new(6, 6, 1), 11).unwrap();
    assert_eq!(
        lot.dirty().floors,
        BTreeSet::from([TilePos::new(1, 1, 1), TilePos::new(6, 6, 1)])
    );
    assert!(lot.dirty().rooms.is_empty());
}

#[test]
fn upstairs_support_uses_enclosed_room_and_source_five_by_five_spread() {
    let mut lot = LotModel::new(8, 8, 3).unwrap();
    let below = TilePos::new(3, 3, 1);
    let upper = TilePos::new(3, 3, 2);
    assert!(!lot.tile(upper).unwrap().supported);
    lot.set_object_support(below, true).unwrap();
    assert!(lot.tile(upper).unwrap().supported);
    let neighbor = TilePos::new(4, 3, 2);
    assert!(!lot.tile(neighbor).unwrap().supported);
    lot.set_floor(upper, 2).unwrap();
    assert!(lot.tile(neighbor).unwrap().supported);
    assert!(!lot.tile(TilePos::new(5, 3, 2)).unwrap().supported);
    lot.set_object_support(below, false).unwrap();
    assert!(!lot.tile(upper).unwrap().supported);
    lot.set_wall(below, WallTile::solid(15)).unwrap();
    assert!(lot.tile(upper).unwrap().supported);
    lot.validate().unwrap();
}

#[test]
fn pool_and_water_partition_terrain_and_portals_connect_levels() {
    let mut world = WorldState::new(LotModel::new(5, 5, 2).unwrap());
    world
        .lot
        .set_floor(TilePos::new(1, 1, 1), tiles::FLOOR_POOL)
        .unwrap();
    world
        .lot
        .set_floor(TilePos::new(2, 1, 1), tiles::FLOOR_WATER)
        .unwrap();
    let pool = world
        .lot
        .rooms()
        .room(world.lot.room_at(TilePos::new(1, 1, 1).center()).unwrap())
        .unwrap();
    assert!(pool.is_pool && !pool.is_water);
    let water = world
        .lot
        .rooms()
        .room(world.lot.room_at(TilePos::new(2, 1, 1).center()).unwrap())
        .unwrap();
    assert!(water.is_water && !water.is_pool);
    let portal = entity(4);
    world
        .insert_object(WorldObject::new(portal, TilePos::new(3, 3, 1).center()))
        .unwrap();
    world
        .lot
        .upsert_portal(Portal {
            id: PortalId(1),
            entity: portal,
            entry: TilePos::new(3, 3, 1).center(),
            exit: TilePos::new(3, 3, 2).center(),
            bidirectional: true,
            enabled: true,
            cost: 1,
            revision: 0,
        })
        .unwrap();
    let graph = world.lot.portal_graph();
    assert_eq!(graph.len(), 2);
    assert_eq!(graph[0].from, graph[1].to);
    assert_ne!(graph[0].from, graph[0].to);
    world.validate().unwrap();
    world.remove_object(portal).unwrap();
    assert!(world.lot.portals().is_empty());
}

#[test]
fn bounds_are_enforced_with_fine_build_masks_and_out_of_world_sentinel() {
    assert!(LotModel::new(0, 5, 1).is_err());
    assert!(LotModel::new(257, 5, 1).is_err());
    assert!(LotModel::new(256, 256, 16).is_err());
    let mut world = WorldState::new(LotModel::new(5, 5, 2).unwrap());
    world
        .lot
        .set_build_bounds(
            BuildBounds {
                min_x: 1,
                min_y: 1,
                max_x: 4,
                max_y: 4,
                max_level: 1,
            },
            Some(BTreeSet::from([(2, 2)])),
        )
        .unwrap();
    assert!(world.lot.buildable(TilePos::new(2, 2, 1)));
    assert!(!world.lot.buildable(TilePos::new(2, 2, 2)));
    assert!(!world.lot.buildable(TilePos::new(1, 1, 1)));
    let outside = WorldObject::new(entity(1), LotPosition::OUT_OF_WORLD);
    assert!(outside.rects().is_empty());
    world.insert_object(outside).unwrap();
    world.validate().unwrap();
    assert_eq!(
        world.insert_object(WorldObject::new(
            entity(2),
            LotPosition::new(-32768, -32767, 1)
        )),
        Err(WorldError::OutOfBounds)
    );
}

#[test]
fn maximal_untrusted_dimensions_are_rejected_before_native_or_wasm_size_arithmetic() {
    assert_eq!(
        LotModel::new(u16::MAX, u16::MAX, u8::MAX),
        Err(LotError::InvalidDimensions)
    );
    let lot = LotModel::new(2, 2, 1).unwrap();
    let mut bytes = bincode::serialize(&lot).unwrap();
    bytes[..5].copy_from_slice(&[255, 255, 255, 255, 255]);
    let malformed: LotModel = bincode::deserialize(&bytes).unwrap();
    assert_eq!(malformed.validate(), Err(LotError::InvalidState));
}
