use std::sync::Arc;
use wonderland_render_core::{Aabb, AssetKey, EntityRef, Mesh, RgbaImage, Vec2, Vec3, Vertex};
use wonderland_world_view::*;

fn source_fixture() -> WorldDocument {
    let mut document = WorldDocument::from_blueprint_xml("<house><size>4</size><world><floors><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/><floor level=\"1\" x=\"1\" y=\"1\" value=\"10\"/></floors><walls/></world><objects/></house>", "tests:source geometry fixture", "fixture-v1").unwrap();
    document.provenance.kind = WorldSourceKind::TestFixture;
    document.source_counts = None;
    document
}
fn cube() -> Mesh {
    let points = [
        (-0.4, 0., -0.4),
        (0.4, 0., -0.4),
        (0.4, 1., -0.4),
        (-0.4, 1., -0.4),
        (-0.4, 0., 0.4),
        (0.4, 0., 0.4),
        (0.4, 1., 0.4),
        (-0.4, 1., 0.4),
    ];
    Mesh {
        vertices: points
            .into_iter()
            .map(|(x, y, z)| Vertex {
                position: Vec3::new(x, y, z),
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                color: [1.; 4],
            })
            .collect(),
        indices: vec![
            0, 1, 2, 2, 3, 0, 4, 6, 5, 6, 4, 7, 0, 4, 5, 5, 1, 0, 3, 2, 6, 6, 7, 3, 1, 5, 6, 6, 2,
            1, 0, 3, 7, 7, 4, 0,
        ],
    }
}
fn with_live_object() -> WorldDocument {
    let mut document = source_fixture();
    document.revision.lot_id = Some(777);
    document.revision.epoch = 12;
    document.revision.tick = 1;
    document.objects.push(WorldObject {
        source_guid: 0x313D2F9A,
        blueprint: None,
        snapshot: None,
        entity: Some(EntityRef {
            object_id: 42,
            generation: 7,
        }),
        visual_revision: 1,
        position_tiles: Vec3::new(1.5, 2., 0.),
        yaw_radians: 0.,
        dynamic_flags: [0; 2],
        room: 0,
        level: 1,
        visible: true,
        selectable: true,
        model: Some(0),
    });
    document.models.push(WorldModel {
        effective_source: AssetKey([27; 32]),
        effective_content: document.revision.content,
        context: ModelContext::Standalone,
        format_version: 2,
        reconstruction_version: 0,
        groups: vec![vec![ModelPart {
            texture: 0,
            mesh: cube(),
        }]],
        textures: vec![ModelTexture {
            selector: ModelTextureSelector::Custom { id: 1 },
            effective_asset: AssetKey([28; 32]),
            uv_scale: Vec2::new(1., 1.),
            image: RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![[220, 20, 20, 255]],
            },
        }],
        bounds: Aabb::new(Vec3::new(-0.4, 0., -0.4), Vec3::new(0.4, 1., 0.4)).unwrap(),
        depth_mask: None,
    });
    document
}
fn object_pick(renderer: &WorldRenderer) -> Option<WorldPick> {
    let image = renderer.image().unwrap();
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            renderer
                .pick(x, y)
                .filter(|pick| matches!(pick.target, WorldPickTarget::Object { .. }))
        })
}

fn repeated_model(count: usize) -> WorldDocument {
    let mut document = with_live_object();
    let object = document.objects[0].clone();
    document.objects = (0..count)
        .map(|index| {
            let mut object = object.clone();
            object.entity.as_mut().unwrap().object_id = index as u32 + 1;
            object
        })
        .collect();
    document
}

// Repeating one source resource must retain one pixel/mesh allocation, not one
// allocation per instance (a 512² image ×1000 objects used to exceed 1 GiB).
#[test]
fn repeated_source_model_instances_share_texture_and_mesh_buffers() {
    let mut document = repeated_model(64);
    document.models[0].textures[0].image = RgbaImage {
        width: 64,
        height: 64,
        pixels: vec![[220, 20, 20, 255]; 64 * 64],
    };
    let scene = build_scene(&document, ViewportControls::default()).unwrap();
    let instances: Vec<_> = scene
        .parts
        .iter()
        .filter(|part| part.object.is_some())
        .collect();
    assert_eq!(instances.len(), 64);
    for instance in &instances[1..] {
        assert!(Arc::ptr_eq(
            instances[0].texture.as_ref().unwrap(),
            instance.texture.as_ref().unwrap()
        ));
        assert_eq!(
            instances[0].mesh.vertices.as_ptr(),
            instance.mesh.vertices.as_ptr()
        );
        assert_eq!(
            instances[0].mesh.indices.as_ptr(),
            instance.mesh.indices.as_ptr()
        );
    }
}

// Each category fits independently; the architecture plus repeated objects
// must share the same checked capacity before any generated meshes are built.
#[test]
fn expanded_scene_budget_includes_architecture_and_all_instance_geometry() {
    let document = repeated_model(4);
    document.validate().unwrap();
    for budget in [
        SceneBudget {
            max_vertices: 90,
            ..Default::default()
        },
        SceneBudget {
            max_indices: 200,
            ..Default::default()
        },
        SceneBudget {
            max_parts: 20,
            ..Default::default()
        },
        SceneBudget {
            max_buffer_bytes: 128,
            ..Default::default()
        },
    ] {
        let error = build_scene_with_budget(&document, ViewportControls::default(), budget)
            .err()
            .expect("aggregate scene capacity must reject before expansion");
        assert!(
            error.0.starts_with("expanded world scene budget:"),
            "{error}"
        );
    }
}

// The validated input owns only 2048 vertices; 1000 instances exceed the total
// scene vertex budget. Reject during preflight, before instance mesh cloning.
#[test]
fn small_unique_model_cannot_expand_past_the_default_scene_budget() {
    let mut document = repeated_model(1000);
    let mesh = &mut document.models[0].groups[0][0].mesh;
    mesh.vertices.resize(2048, mesh.vertices[0]);
    document.validate().unwrap();
    let error = build_scene(&document, ViewportControls::default())
        .err()
        .expect("large repeated geometry must be rejected before expansion");
    assert_eq!(error.0, "expanded world scene budget: vertices");
}

// Catches floor controls hiding DOM only, wrong one-based levels, and loss of
// the 3x graphics conversion from source's 2.95 story height.
#[test]
fn floor_and_wall_modes_change_the_generated_source_meshes() {
    let mut document = source_fixture();
    document.lot.tiles[2 * 4 + 1].wall = WorldWall {
        north: true,
        styles: [0, 1],
        ..Default::default()
    };
    let mut controls = ViewportControls::default();
    let up = build_scene(&document, controls).unwrap();
    assert!(
        !up.parts
            .iter()
            .any(|part| part.tile.is_some_and(|(_, _, level)| level == 2))
    );
    let wall_max = |scene: &PreparedWorld| {
        scene
            .parts
            .iter()
            .filter(|part| part.surface == Some(WorldSurface::Wall))
            .flat_map(|part| &part.mesh.vertices)
            .map(|vertex| vertex.position.y)
            .fold(0., f32::max)
    };
    assert!((wall_max(&up) - 8.85).abs() < 0.001);
    controls.walls = WallMode::Down;
    let down = build_scene(&document, controls).unwrap();
    assert!(wall_max(&down) < wall_max(&up) / 2.);
    controls.visible_level = 2;
    let level_two = build_scene(&document, controls).unwrap();
    let floor = level_two
        .parts
        .iter()
        .find(|part| part.tile == Some((1, 1, 2)) && part.surface == Some(WorldSurface::Floor))
        .unwrap();
    assert!(
        floor
            .mesh
            .vertices
            .iter()
            .all(|vertex| (vertex.position.y - 8.85).abs() < 0.001)
    );
}

// Catches silently recomputing room/cutaway state from decorative floors.
#[test]
fn cutaway_uses_the_supplied_source_map_and_reports_an_unavailable_map() {
    let mut document = source_fixture();
    document.lot.tiles[2 * 4 + 1].wall = WorldWall {
        north: true,
        styles: [0, 1],
        ..Default::default()
    };
    let controls = ViewportControls {
        walls: WallMode::Cutaway,
        ..Default::default()
    };
    let absent = build_scene(&document, controls).unwrap();
    assert!(
        absent
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_cutaway_map")
    );
    document.lot.cutaway = Some(vec![true; 4 * 4 * 5]);
    let present = build_scene(&document, controls).unwrap();
    assert!(
        !present
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_cutaway_map")
    );
    let high = |scene: &PreparedWorld| {
        scene
            .parts
            .iter()
            .filter(|part| part.surface == Some(WorldSurface::Wall))
            .flat_map(|part| &part.mesh.vertices)
            .any(|vertex| vertex.position.y > 5.)
    };
    assert!(high(&absent));
    assert!(!high(&present));
}

// Catches a decorative rotation disconnected from the actual source camera.
#[test]
fn camera_turn_and_pan_change_source_projection_and_reject_nonfinite_controls() {
    let document = source_fixture();
    let controls = ViewportControls::default();
    let matrix = camera_projection(&document, controls, 4. / 3.).unwrap();
    let centre = matrix.transform_vec4([6., 3., 6., 1.]);
    assert!(centre[0].abs() < 0.0001 && centre[1].abs() < 0.0001);
    let point = matrix.transform_vec4([3., 0., 6., 1.]);
    let turned = camera_projection(
        &document,
        ViewportControls {
            yaw_radians: controls.yaw_radians + std::f32::consts::FRAC_PI_2,
            pan_x: 1.,
            ..controls
        },
        4. / 3.,
    )
    .unwrap()
    .transform_vec4([3., 0., 6., 1.]);
    assert!((point[0] / point[3] - turned[0] / turned[3]).abs() > 0.05);
    assert!(
        camera_projection(
            &document,
            ViewportControls {
                zoom: f32::NAN,
                ..controls
            },
            4. / 3.
        )
        .is_err()
    );
}

// Catches fake tile EntityRefs entering FrameStore or color pixels diverging
// from the private depth pick buffer after a camera turn.
#[test]
fn software_pixels_pick_actual_tiles_and_live_entity_generations() {
    let document = Arc::new(with_live_object());
    let mut renderer = WorldRenderer::new(document).unwrap();
    for yaw in [-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4] {
        renderer
            .render(
                ViewportControls {
                    yaw_radians: yaw,
                    ..Default::default()
                },
                256,
                192,
            )
            .unwrap();
        let picked = object_pick(&renderer).expect("visible source mesh is pickable");
        assert!(matches!(
            picked.target,
            WorldPickTarget::Object {
                entity: Some(EntityRef {
                    object_id: 42,
                    generation: 7
                }),
                source_guid: 0x313D2F9A,
                ..
            }
        ));
        assert_eq!(picked.revision.lot_id, Some(777));
        let mut tiles = vec![];
        for y in 0..192 {
            for x in 0..256 {
                if let Some(WorldPick {
                    target: WorldPickTarget::Tile { x, y, level, .. },
                    ..
                }) = renderer.pick(x, y)
                {
                    tiles.push((x, y, level));
                }
            }
        }
        assert!(!tiles.is_empty());
        assert!(
            tiles
                .iter()
                .all(|&(x, y, level)| x < 4 && y < 4 && level == 1)
        );
    }
}

// Catches painter-order hit testing or an ownerless wall failing to clear the
// ID of a hidden object. Down mode must change occlusion in the same buffers.
#[test]
fn source_ownerless_walls_occlude_objects_until_cut_down() {
    let mut document = with_live_object();
    for x in 0..4 {
        document.lot.tiles[4 + x].wall = WorldWall {
            north: true,
            styles: [0, 1],
            ..Default::default()
        };
    }
    let mut renderer = WorldRenderer::new(Arc::new(document)).unwrap();
    let controls = ViewportControls {
        yaw_radians: std::f32::consts::FRAC_PI_2,
        pitch_radians: 0.,
        ..Default::default()
    };
    renderer.render(controls, 256, 192).unwrap();
    assert!(
        object_pick(&renderer).is_none(),
        "ownerless wall must win depth and picking"
    );
    renderer
        .render(
            ViewportControls {
                walls: WallMode::Down,
                ..controls
            },
            256,
            192,
        )
        .unwrap();
    assert!(object_pick(&renderer).is_some());
}

// Catches old hit results reviving after device loss, camera redraw, new frames,
// or live object removal and reuse of the same generation.
#[test]
fn stale_world_picks_and_reused_entity_generations_are_rejected() {
    let document = with_live_object();
    let mut renderer = WorldRenderer::new(Arc::new(document.clone())).unwrap();
    renderer.render(Default::default(), 256, 192).unwrap();
    let picked = object_pick(&renderer).unwrap();
    assert!(renderer.resolve_pick(&picked).is_some());
    renderer.device_reset();
    assert!(renderer.resolve_pick(&picked).is_none());
    renderer.render(Default::default(), 256, 192).unwrap();
    let picked = object_pick(&renderer).unwrap();
    let mut removed = document.clone();
    removed.revision.tick = 2;
    removed.objects.clear();
    renderer.replace_document(Arc::new(removed)).unwrap();
    assert!(renderer.resolve_pick(&picked).is_none());
    let mut reused = document.clone();
    reused.revision.tick = 3;
    assert!(renderer.replace_document(Arc::new(reused)).is_err());
    let mut replacement = document;
    replacement.revision.tick = 3;
    replacement.objects[0].entity.as_mut().unwrap().generation = 8;
    renderer.replace_document(Arc::new(replacement)).unwrap();
    renderer.render(Default::default(), 256, 192).unwrap();
    assert!(matches!(
        object_pick(&renderer).unwrap().target,
        WorldPickTarget::Object {
            entity: Some(EntityRef { generation: 8, .. }),
            ..
        }
    ));
}

// Catches a live producer changing accepted architecture or dynamic mesh groups
// without advancing the source revision which validates existing pick tickets.
#[test]
fn live_architecture_and_dynamic_visual_changes_require_their_revisions() {
    let document = with_live_object();
    let mut renderer = WorldRenderer::new(Arc::new(document.clone())).unwrap();
    let mut changed = document.clone();
    changed.revision.tick = 2;
    changed.lot.tiles[0].floor = 10;
    assert!(
        renderer
            .replace_document(Arc::new(changed.clone()))
            .is_err()
    );
    changed.revision.architecture_revision = 1;
    renderer
        .replace_document(Arc::new(changed.clone()))
        .unwrap();
    changed.revision.tick = 3;
    changed.objects[0].dynamic_flags[0] = 1;
    assert!(
        renderer
            .replace_document(Arc::new(changed.clone()))
            .is_err()
    );
    changed.objects[0].visual_revision = 2;
    renderer.replace_document(Arc::new(changed)).unwrap();
}

// Refresh-only snapshots have no authoritative architecture/entity generations;
// the explicit presentation generation still prevents old geometry/pick revival.
#[test]
fn snapshot_refresh_requires_a_new_presentation_generation() {
    let mut document = source_fixture();
    document.provenance.kind = WorldSourceKind::LegacySnapshot;
    document.revision.architecture_revision = 1;
    let mut renderer = WorldRenderer::new(Arc::new(document.clone())).unwrap();
    renderer.render(Default::default(), 128, 96).unwrap();
    let pick = (0..96)
        .flat_map(|y| (0..128).map(move |x| (x, y)))
        .find_map(|(x, y)| renderer.pick(x, y))
        .unwrap();
    let mut changed = document.clone();
    changed.lot.tiles[5].floor = 12;
    assert!(
        renderer
            .replace_document(Arc::new(changed.clone()))
            .is_err()
    );
    assert!(renderer.resolve_pick(&pick).is_some());
    changed.revision.architecture_revision = 2;
    renderer.replace_document(Arc::new(changed)).unwrap();
    assert!(renderer.resolve_pick(&pick).is_none());
    assert!(renderer.replace_document(Arc::new(document)).is_err());
}

// Catches rendering the original 77-square source with the fixture geometry,
// dropping the source floor material behind coplanar grass, or losing tile IDs.
#[test]
fn complete_original_empty_lot_renders_source_tiles_with_explicit_missing_resources() {
    let document = Arc::new(WorldDocument::original_empty_lot().unwrap());
    let mut renderer = WorldRenderer::new(document).unwrap();
    let stats = renderer
        .render(ViewportControls::default(), 384, 256)
        .unwrap();
    assert_eq!(stats.triangles, 14_396);
    let image = renderer.image().unwrap();
    let mut source_floor = 0;
    let mut source_grass = 0;
    for y in 0..image.height {
        for x in 0..image.width {
            match renderer.pick(x, y).map(|pick| pick.target) {
                Some(WorldPickTarget::Tile {
                    surface: WorldSurface::Floor,
                    ..
                }) => source_floor += 1,
                Some(WorldPickTarget::Tile {
                    surface: WorldSurface::Terrain,
                    ..
                }) => source_grass += 1,
                Some(WorldPickTarget::Object { .. }) => {
                    panic!("unavailable original meshes must not become invented objects")
                }
                _ => {}
            }
        }
    }
    assert!(source_floor > 100 && source_grass > 1000);
    assert!(
        stats
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_object_model")
    );
}
