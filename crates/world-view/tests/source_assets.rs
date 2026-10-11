use wonderland_world_view::*;

// Catches using the synthetic pool bowl, dropping authored OBJ faces, omitting
// Modelled3DFloorTile's +0.5 offset, or failing to flip source texture V.
#[test]
fn authored_pool_obj_and_texture_are_preserved() {
    let (assets, texture) = source_pool_assets().unwrap();
    assert_eq!((assets.tiles.len(), assets.corners.len()), (16, 4));
    assert_eq!(assets.tiles[0].indices.len(), 456 * 3);
    assert_eq!(assets.tiles[15].indices.len(), 4 * 3);
    assert_eq!(assets.corners[0].indices.len(), 72 * 3);
    assert_eq!(
        (texture.width, texture.height, texture.pixels.len()),
        (512, 256, 131072)
    );
    let first = &assets.tiles[15].vertices[0];
    assert!((first.position.x - 0.5).abs() < 0.000001);
    assert!((first.position.z - 0.5).abs() < 0.000001);
    assert!((first.position.y + 0.073719).abs() < 0.000001);
    assert!((first.uv.y - 0.456942).abs() < 0.000001);
    assert!(parse_source_pool_obj("v 0 0 0\nvt 0 0\nvn 0 1 0\nf 1/1/1 2/1/1 3/1/1").is_err());
    assert!(parse_source_pool_obj("v NaN 0 0\nvt 0 0\nvn 0 1 0\nf 1/1/1 1/1/1 1/1/1").is_err());
}

// Catches loading pool assets but retaining terrain over the pool's authored
// below-ground bowl, and losing the actual pool texture at material lookup.
#[test]
fn original_pool_geometry_replaces_ground_and_uses_its_source_texture() {
    let document = WorldDocument::from_blueprint_xml("<house><size>5</size><world><floors/><walls/><pools><pool x=\"2\" y=\"2\" value=\"1\"/></pools></world></house>","tests:pool fixture","fixture-v1").unwrap();
    let scene = build_scene(&document, ViewportControls::default()).unwrap();
    assert!(
        !scene
            .diagnostics
            .iter()
            .any(|entry| entry.code == "missing_architecture_resource")
    );
    let pool = scene
        .parts
        .iter()
        .find(|part| part.surface == Some(WorldSurface::Pool))
        .unwrap();
    assert!(pool.texture.is_some());
    assert_eq!(pool.mesh.indices.len(), (456 + 4) * 3);
    assert!(!scene.parts.iter().any(|part| part.surface == Some(WorldSurface::Terrain) && part.tile == Some((2,2,1))));
}

// Catches a roof toggle disconnected from actual source roof generation.
#[test]
fn roof_toggle_controls_source_roof_geometry_with_supplied_room_state() {
    let mut document = WorldDocument::from_blueprint_xml(
        "<house><size>5</size><world><floors/><walls/></world></house>",
        "tests:roof fixture",
        "fixture-v1",
    )
    .unwrap();
    for y in 1..4 {
        for x in 1..4 {
            document.lot.tiles[y * 5 + x].indoors = Some(true);
        }
    }
    document.lot.roof = Some(WorldRoof {
        material: 17,
        pitch: 0.5,
        advanced: true,
        average_color: [1.; 4],
        texture_scale: 1.,
    });
    let off = build_scene(&document, ViewportControls::default()).unwrap();
    let on = build_scene(
        &document,
        ViewportControls {
            show_roofs: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !off.parts
            .iter()
            .any(|part| part.surface == Some(WorldSurface::Roof))
    );
    assert!(
        on.parts
            .iter()
            .any(|part| part.surface == Some(WorldSurface::Roof))
    );
}
