//! Renderer picking contract only; source offers and routing remain runtime-owned.
use wonderland_game_runtime::LotPosition;
use wonderland_web_shell::native_terrain::native_pick_terrain;
use wonderland_world_view::{
    WorldDocument, WorldPick, WorldPickTarget, WorldSourceKind, WorldSurface,
};

fn scene() -> (WorldDocument, WorldPick) {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:terrain-pick",
        "authored fixture",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::LiveSession;
    world.revision.lot_id = Some(11);
    world.revision.epoch = 7;
    world.revision.tick = 9;
    let pick = WorldPick {
        revision: world.revision,
        frame_generation: 1,
        target: WorldPickTarget::Tile {
            x: 2,
            y: 3,
            level: 1,
            surface: WorldSurface::Terrain,
        },
        screen: [20, 20],
    };
    (world, pick)
}
#[test]
fn terrain_pick_uses_source_subtile_center_without_moving_any_entity() {
    let (mut world, pick) = scene();
    let before = world.clone();
    assert_eq!(
        native_pick_terrain(&world, &pick),
        Some(LotPosition {
            x: 40,
            y: 56,
            level: 1
        })
    );
    assert_eq!(world, before);
    // Routine tick delivery does not invalidate unchanged terrain geometry.
    world.revision.tick += 1;
    assert!(native_pick_terrain(&world, &pick).is_some());
}
#[test]
fn terrain_pick_requires_current_native_identity_and_architecture() {
    let (world, pick) = scene();
    for mutation in 0..7 {
        let mut stale = world.clone();
        match mutation {
            0 => stale.provenance.kind = WorldSourceKind::TestFixture,
            1 => stale.revision.lot_id = Some(12),
            2 => stale.revision.epoch += 1,
            3 => stale.revision.content.0[0] ^= 1,
            4 => stale.revision.architecture_revision += 1,
            5 => stale.revision.tick -= 1,
            _ => stale.revision.lot_id = None,
        }
        assert!(
            native_pick_terrain(&stale, &pick).is_none(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn terrain_pick_rejects_unsupported_surfaces_levels_bounds_and_objects() {
    let (world, mut pick) = scene();
    for surface in [
        WorldSurface::Wall,
        WorldSurface::WallTop,
        WorldSurface::Roof,
        WorldSurface::BuildSupport,
    ] {
        pick.target = WorldPickTarget::Tile {
            x: 2,
            y: 3,
            level: 1,
            surface,
        };
        assert!(native_pick_terrain(&world, &pick).is_none());
    }
    for (x, y, level) in [
        (8, 3, 1),
        (2, 8, 1),
        (2, 3, 0),
        (2, 3, 127),
        (u16::MAX, 3, 1),
    ] {
        pick.target = WorldPickTarget::Tile {
            x,
            y,
            level,
            surface: WorldSurface::Floor,
        };
        assert!(native_pick_terrain(&world, &pick).is_none());
    }
    pick.target = WorldPickTarget::Object {
        entity: None,
        source_guid: 1,
        source_record: None,
    };
    assert!(native_pick_terrain(&world, &pick).is_none());
    pick.target = WorldPickTarget::Tile {
        x: 2,
        y: 3,
        level: 1,
        surface: WorldSurface::Floor,
    };
    assert!(native_pick_terrain(&world, &pick).is_some());
}

#[test]
fn pool_and_water_picks_leave_source_menu_checks_in_charge() {
    let (world, mut pick) = scene();
    for surface in [WorldSurface::Pool, WorldSurface::Water] {
        pick.target = WorldPickTarget::Tile {
            x: 2,
            y: 3,
            level: 1,
            surface,
        };
        assert_eq!(
            native_pick_terrain(&world, &pick),
            Some(LotPosition {
                x: 40,
                y: 56,
                level: 1
            })
        );
    }
}
