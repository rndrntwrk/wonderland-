use std::sync::Arc;
use wonderland_render_core::{AssetKey, Vec2};
use wonderland_world_view::*;
#[path = "support/lighting.rs"]
mod support;
use support::lit_world;

#[test]
fn source_room_and_shadow_pixels_are_built_from_the_declared_recipe() {
    let world = lit_world();
    world.validate().unwrap();
    let light = world.lighting.as_ref().unwrap();
    let prepared = light.prepare(&world).unwrap();
    assert_eq!([prepared.image().width, prepared.image().height], [48, 32]);
    assert!(prepared.image().pixels[8 * 48 + 3][0] > 50);
    assert_eq!(prepared.image().pixels[8 * 48 + 12], [0; 4]);
    let mut changed = world.clone();
    changed.lighting.as_mut().unwrap().geometry[0].walls.clear();
    let changed = changed
        .lighting
        .as_ref()
        .unwrap()
        .prepare(&changed)
        .unwrap();
    assert_ne!(prepared.key(), changed.key());
    assert_ne!(prepared.image(), changed.image());
}

#[test]
fn malformed_even_unused_geometry_and_foreign_rooms_fail_at_admission() {
    let mut variants = Vec::new();
    let mut change = |edit: fn(&mut WorldLighting)| {
        let mut world = lit_world();
        edit(world.lighting.as_mut().unwrap());
        variants.push(world);
    };
    change(|l| l.geometry[0].walls[0][0].x = f32::NAN);
    change(|l| l.geometry[0].walls[0] = [Vec2::ZERO; 2]);
    change(|l| l.geometry[0].walls[0][0].x = 1_000_001.);
    change(|l| {
        l.lights.clear();
        l.geometry[0].room = 77;
    });
    change(|l| l.geometry[0].floor = 1);
    change(|l| l.geometry.push(l.geometry[0].clone()));
    change(|l| l.geometry[0].objects.push([0., 0., 0., 10.]));
    change(|l| l.geometry[0].objects.push([0., 0., 1.5, 10.]));
    change(|l| l.geometry[0].objects.push([0., f32::INFINITY, 1., 10.]));
    change(|l| l.lights[0].room = 77);
    change(|l| l.lights[0].floor = 1);
    change(|l| l.lights[0].window_room = Some(77));
    change(|l| l.lights.push(l.lights[0]));
    change(|l| l.lights[0].intensity = f32::INFINITY);
    change(|l| l.lights[0].radius_sixteenths = 0.);
    change(|l| l.cells[0].first = 77);
    change(|l| l.rooms.push(l.rooms[0]));
    change(|l| l.rooms[0].floor = 1);
    change(|l| l.source = AssetKey([9; 32]));
    change(|l| l.epoch += 1);
    change(|l| l.lot_id += 1);
    change(|l| l.width += 1);
    change(|l| {
        l.cells.pop();
    });
    for (index, world) in variants.iter().enumerate() {
        assert!(
            world.validate().is_err(),
            "invalid lighting case {index} was admitted"
        );
        assert!(WorldRenderer::new(Arc::new(world.clone())).is_err());
    }
}

#[test]
fn lighting_is_not_silently_downgraded_to_the_previous_document_schema() {
    let mut world = lit_world();
    world.schema_version = WORLD_SCHEMA_VERSION;
    assert!(world.validate().is_err());
    let value = serde_json::to_value(lit_world()).unwrap();
    assert!(value["lighting"]["lot_id"].is_string());
    assert!(value["lighting"]["lights"][0]["id"].is_string());
    let restored: WorldDocument = serde_json::from_value(value).unwrap();
    restored.validate().unwrap();
}

#[test]
fn actual_reference_and_gpu_paths_use_light_pixels_not_just_metadata() {
    let world = lit_world();
    let mut lit = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    let mut unlit = world;
    unlit.lighting = None;
    unlit.schema_version = WORLD_SCHEMA_VERSION;
    let mut unlit = WorldRenderer::new(Arc::new(unlit)).unwrap();
    lit.render(Default::default(), 160, 120).unwrap();
    unlit.render(Default::default(), 160, 120).unwrap();
    assert_ne!(
        lit.image(),
        unlit.image(),
        "light recipe never reached actual raster"
    );
    for y in 0..120 {
        for x in 0..160 {
            assert_eq!(
                lit.pick(x, y).map(|p| p.target),
                unlit.pick(x, y).map(|p| p.target),
                "lighting changed a selection at {x},{y}"
            );
        }
    }
    let (packet, _) = lit.prepare_gpu(Default::default(), 160, 120).unwrap();
    assert_eq!(
        packet.schema, 3,
        "GPU host cannot silently ignore a lightmap"
    );
    let json = serde_json::to_value(&packet).unwrap();
    assert!(
        json["draws"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["light"].is_object())
    );
    for y in 0..120 {
        for x in 0..160 {
            // Compare the unlit CPU target after rendering a new lit CPU frame below.
            if let Some(p) = unlit.pick(x, y) {
                assert!(matches!(p.target, WorldPickTarget::Tile { .. }));
            }
        }
    }
}

#[test]
fn changed_light_bytes_need_a_new_revision_and_failed_replacement_preserves_picks() {
    let world = lit_world();
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    let id = frame.draws.iter().find(|d| d.pick_id > 0).unwrap().pick_id;
    let generation = frame.generation.parse().unwrap();
    let before = renderer.resolve_gpu_pick(generation, id, 64, 48).unwrap();
    let mut invalid = world.clone();
    invalid.lighting.as_mut().unwrap().geometry[0].walls[0][0].x = f32::NAN;
    assert!(renderer.replace_document(Arc::new(invalid)).is_err());
    assert_eq!(
        renderer.resolve_gpu_pick(generation, id, 64, 48),
        Some(before.clone())
    );
    let mut changed = world.clone();
    changed.revision.tick += 1;
    changed.lighting.as_mut().unwrap().minimum = [200; 4];
    assert!(
        renderer
            .replace_document(Arc::new(changed.clone()))
            .is_err(),
        "changed light bytes reused a revision"
    );
    assert_eq!(
        renderer.resolve_gpu_pick(generation, id, 64, 48),
        Some(before)
    );
    changed.lighting.as_mut().unwrap().revision += 1;
    renderer.replace_document(Arc::new(changed)).unwrap();
    assert!(renderer.resolve_gpu_pick(generation, id, 64, 48).is_none());
}

#[test]
fn tick_only_updates_and_device_reset_reuse_the_validated_light_atlas() {
    let world = lit_world();
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    assert_eq!(renderer.lighting_preparations(), 1);
    renderer.render(Default::default(), 128, 96).unwrap();
    let mut next = world.clone();
    next.revision.tick += 1;
    renderer.replace_document(Arc::new(next.clone())).unwrap();
    renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    renderer.device_reset();
    renderer.render(Default::default(), 128, 96).unwrap();
    assert_eq!(renderer.lighting_preparations(), 1);
    next.revision.tick += 1;
    next.lighting.as_mut().unwrap().revision += 1;
    next.lighting.as_mut().unwrap().minimum = [50; 4];
    renderer.replace_document(Arc::new(next)).unwrap();
    assert_eq!(renderer.lighting_preparations(), 2);
}

#[test]
fn tileless_upper_roofs_sample_their_source_floor_not_the_ground_floor() {
    let mut world = lit_world();
    let larger = WorldDocument::from_blueprint_xml(
        "<house><size>5</size><world><floors/><walls/></world><objects/></house>",
        "test:roof-light",
        "fixture",
    )
    .unwrap();
    world.lot = larger.lot;
    let light = world.lighting.as_mut().unwrap();
    light.width = 5;
    light.height = 5;
    light.cells = (0..world.lot.levels)
        .flat_map(|f| {
            vec![
                WorldLightCell {
                    first: u16::from(f) + 1,
                    second: u16::from(f) + 1,
                    diagonal: LightRoomDiagonal::None,
                    floor_pattern: 0,
                };
                25
            ]
        })
        .collect();
    let width = usize::from(world.lot.width);
    let floor_area = width * usize::from(world.lot.height);
    assert!(world.lot.levels >= 2);
    for y in 1..4 {
        for x in 1..4 {
            world.lot.tiles[floor_area + y * width + x].indoors = Some(true);
        }
    }
    world.lot.roof = Some(WorldRoof {
        material: 17,
        pitch: 0.5,
        advanced: true,
        average_color: [1.; 4],
        texture_scale: 1.,
    });
    let controls = ViewportControls {
        visible_level: 2,
        show_roofs: true,
        ..Default::default()
    };
    let scene = build_scene(&world, controls).unwrap();
    let upper_roof = scene
        .parts
        .iter()
        .find(|p| p.level > 1 && p.tile.is_none() && p.surface == Some(WorldSurface::Roof))
        .expect("fixture must include a tileless upper roof");
    let light = world.lighting.as_ref().unwrap().prepare(&world).unwrap();
    let desired = light
        .model_to_uv(upper_roof.transform, upper_roof.level - 1)
        .unwrap();
    let desired: [f32; 16] = std::array::from_fn(|i| desired.cols[i / 4][i % 4]);
    let ground = light.model_to_uv(upper_roof.transform, 0).unwrap();
    let ground: [f32; 16] = std::array::from_fn(|i| ground.cols[i / 4][i % 4]);
    assert_ne!(
        desired, ground,
        "fixture must distinguish floor atlas cells"
    );
    let mut renderer = WorldRenderer::new(Arc::new(world)).unwrap();
    let (packet, _) = renderer.prepare_gpu(controls, 160, 120).unwrap();
    assert!(
        packet
            .draws
            .iter()
            .any(|d| d.pick_id == 0 && d.light.as_ref().is_some_and(|l| l.matrix == desired)),
        "upper roof incorrectly uses the ground-floor light atlas"
    );
}
