use wonderland_game_runtime::live_wire::player::PlayerBinding;
use wonderland_game_runtime::{Facing, TilePos};
use wonderland_web_shell::native_build::*;
use wonderland_world_view::{WorldDocument, WorldPick, WorldPickTarget, WorldSurface};
fn world() -> WorldDocument {
    let mut w = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:build",
        "declared fixture",
    )
    .unwrap();
    w.revision.lot_id = Some(11);
    w.revision.epoch = 7;
    w.revision.tick = 3;
    w.revision.architecture_revision = 0;
    w.revision.content.0 = [9; 32];
    w
}
fn catalogue() -> Catalogue {
    Catalogue {
        version: 1,
        binding: PlayerBinding {
            source_epoch: 1,
            lot_incarnation: 2,
            lot_location: 3,
            avatar_id: 42,
        },
        lot_id: 11,
        authority_epoch: 7,
        content_hash: [9; 32],
        permissions_revision: 1,
        account_revision: 2,
        catalog_revision: 3,
        balance: 1000,
        can_build: true,
        next_request_id: 1,
        csrf_token: "test-only-token".into(),
        items: vec![
            Item {
                kind: ItemKind::Object { catalog_id: 5 },
                label: "Desk".into(),
                category: "Surfaces".into(),
                price: 200,
                thumbnail: None,
            },
            Item {
                kind: ItemKind::Floor { pattern: 1 },
                label: "Stone".into(),
                category: "Floors".into(),
                price: 7,
                thumbnail: None,
            },
        ],
    }
}
fn pick(w: &WorldDocument) -> WorldPick {
    WorldPick {
        revision: w.revision,
        frame_generation: 1,
        target: WorldPickTarget::Tile {
            x: 2,
            y: 3,
            level: 1,
            surface: WorldSurface::Terrain,
        },
        screen: [12, 20],
    }
}
#[test]
fn valid_catalogue_keeps_full_width_revisions() {
    let mut c = catalogue();
    c.account_revision = u64::MAX;
    let bytes = serde_json::to_vec(&c).unwrap();
    let parsed = Catalogue::decode(&bytes, c.binding, &world()).unwrap();
    assert_eq!(parsed.account_revision, u64::MAX);
}
#[test]
fn another_lot_or_actor_cannot_enable_construction() {
    let c = catalogue();
    let mut b = c.binding;
    b.avatar_id += 1;
    assert!(Catalogue::decode(&serde_json::to_vec(&c).unwrap(), b, &world()).is_err());
    let mut w = world();
    w.revision.lot_id = Some(12);
    assert!(c.validate(c.binding, &w).is_err());
}
#[test]
fn wrong_content_and_epoch_are_not_accepted() {
    let c = catalogue();
    let mut w = world();
    w.revision.content.0[0] ^= 1;
    assert!(c.validate(c.binding, &w).is_err());
    w = world();
    w.revision.epoch += 1;
    assert!(c.validate(c.binding, &w).is_err());
}
#[test]
fn duplicates_and_negative_prices_reject_whole_catalogue() {
    let mut c = catalogue();
    c.items.push(c.items[0].clone());
    assert!(c.validate(c.binding, &world()).is_err());
    c = catalogue();
    c.items[0].price = -1;
    assert!(c.validate(c.binding, &world()).is_err());
}
#[test]
fn empty_is_real_empty_but_oversized_metadata_rejects() {
    let mut c = catalogue();
    c.items.clear();
    assert!(c.validate(c.binding, &world()).is_ok());
    c = catalogue();
    c.items[0].label = "x".repeat(161);
    assert!(c.validate(c.binding, &world()).is_err());
    assert!(Catalogue::decode(&vec![b' '; MAX_CATALOGUE_BYTES + 1], c.binding, &world()).is_err());
}
#[test]
fn thumbnails_cannot_fetch_foreign_origins_or_traverse() {
    for p in [
        "https://foreign.invalid/x.png",
        "//foreign/x",
        "/a/../b.png",
        "/a%2fb.png",
        "/file?token=x",
        "/file#x",
    ] {
        let mut c = catalogue();
        c.items[0].thumbnail = Some(p.into());
        assert!(c.validate(c.binding, &world()).is_err(), "{p}");
    }
    let mut c = catalogue();
    c.items[0].thumbnail = Some("/assets/catalogue/desk.png".into());
    assert!(c.validate(c.binding, &world()).is_ok());
}
#[test]
fn invalid_csrf_and_zero_request_identity_reject() {
    let mut c = catalogue();
    c.csrf_token = "abc\r\nX:evil".into();
    assert!(c.validate(c.binding, &world()).is_err());
    c = catalogue();
    c.next_request_id = 0;
    assert!(c.validate(c.binding, &world()).is_err());
}
#[test]
fn unknown_fields_are_not_silently_admitted() {
    let c = catalogue();
    let mut v = serde_json::to_value(&c).unwrap();
    v["owner_grant"] = serde_json::json!(true);
    assert!(Catalogue::decode(&serde_json::to_vec(&v).unwrap(), c.binding, &world()).is_err());
}
#[test]
fn valid_scene_tile_builds_choice_not_local_edit() {
    let c = catalogue();
    let w = world();
    let mut d = Draft::default();
    d.choose(c.items[0].clone());
    d.pick(&pick(&w), &w).unwrap();
    let request = d.request(&c, &w, 8).unwrap();
    assert_eq!(
        request.selections,
        vec![
            wonderland_game_runtime::live_wire::construction::Selection::Purchase {
                catalog_id: 5,
                position: TilePos::new(2, 3, 1).center(),
                facing: Facing::NORTH
            }
        ]
    );
    assert_eq!(w.revision.architecture_revision, 0);
    assert!(w.objects.is_empty());
}
#[test]
fn stale_architecture_future_tick_and_wrong_content_pick_reject() {
    let w = world();
    for case in 0..3 {
        let mut p = pick(&w);
        match case {
            0 => p.revision.architecture_revision += 1,
            1 => p.revision.tick += 1,
            _ => p.revision.content.0[0] ^= 1,
        }
        assert!(Draft::default().pick(&p, &w).is_err());
    }
}
#[test]
fn out_of_range_and_wall_surface_are_not_floor_tiles() {
    let w = world();
    let mut p = pick(&w);
    p.target = WorldPickTarget::Tile {
        x: 8,
        y: 0,
        level: 1,
        surface: WorldSurface::Terrain,
    };
    assert!(Draft::default().pick(&p, &w).is_err());
    p.target = WorldPickTarget::Tile {
        x: 1,
        y: 0,
        level: 0,
        surface: WorldSurface::Terrain,
    };
    assert!(Draft::default().pick(&p, &w).is_err());
    for surface in [
        WorldSurface::Wall,
        WorldSurface::WallTop,
        WorldSurface::Water,
        WorldSurface::Pool,
        WorldSurface::Roof,
    ] {
        p.target = WorldPickTarget::Tile {
            x: 1,
            y: 1,
            level: 1,
            surface,
        };
        assert!(Draft::default().pick(&p, &w).is_err());
    }
}
#[test]
fn ordinary_tick_does_not_break_valid_picking() {
    let mut w = world();
    let p = pick(&w);
    w.revision.tick += 10;
    let mut d = Draft::default();
    d.pick(&p, &w).unwrap();
    assert_eq!(d.tile, Some(TilePos::new(2, 3, 1)));
}
#[test]
fn selecting_rotating_or_changing_catalogue_never_confirms() {
    let c = catalogue();
    let w = world();
    let mut d = Draft::default();
    d.choose(c.items[0].clone());
    assert!(d.request(&c, &w, 1).is_err());
    d.pick(&pick(&w), &w).unwrap();
    d.rotate();
    let req = d.request(&c, &w, 1).unwrap();
    assert!(matches!(
        req.selections[0],
        wonderland_game_runtime::live_wire::construction::Selection::Purchase {
            facing: Facing::EAST,
            ..
        }
    ));
    let mut withdrawn = c.clone();
    withdrawn.items.clear();
    assert!(d.request(&withdrawn, &w, 2).is_err());
}
#[test]
fn no_build_rights_cannot_prepare_even_zero_price_item() {
    let mut c = catalogue();
    c.can_build = false;
    c.items[0].price = 0;
    let w = world();
    let mut d = Draft::default();
    d.choose(c.items[0].clone());
    d.pick(&pick(&w), &w).unwrap();
    assert!(d.request(&c, &w, 1).is_err());
}
#[test]
fn outline_samples_accepted_terrain_without_modifying_world() {
    let w = world();
    let mut d = Draft::default();
    d.pick(&pick(&w), &w).unwrap();
    let outline = d.outline(&w);
    assert_eq!(outline.points.len(), 4);
    assert!(outline.closed);
    assert_eq!(outline.points[0].x, 6.);
    assert_eq!(outline.points[0].z, 9.);
}
#[test]
fn filtering_and_pagination_are_bounded() {
    let c = catalogue();
    assert_eq!(c.page("desk", "", 0, 12).len(), 1);
    assert_eq!(c.page("", "Floors", 0, 12)[0].label, "Stone");
    assert!(c.page("", "", usize::MAX, 12).is_empty());
    assert!(c.page("", "", 0, 0).is_empty());
}
#[test]
fn diagnostic_format_never_discloses_the_csrf_capability() {
    let c = catalogue();
    assert!(!format!("{c:?}").contains("test-only-token"));
}
