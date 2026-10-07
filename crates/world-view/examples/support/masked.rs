use super::*;
use wonderland_render_core::{Aabb, AssetKey, EntityRef, Mesh, RgbaImage, Vec2, Vec3, Vertex};
pub fn cube() -> Mesh {
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

pub fn masked(kind: ModelMaskKind) -> WorldDocument {
    let mut document = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors/><walls/></world><objects/></house>",
        match kind {
            ModelMaskKind::Normal => "tests:source normal mask fixture",
            ModelMaskKind::Portal => "tests:source portal mask fixture",
        },
        "mask-v1",
    )
    .unwrap();
    document.provenance.kind = WorldSourceKind::TestFixture;
    document.source_counts = None;
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
    let body = cube();
    let mut mask = cube();
    for v in &mut mask.vertices {
        v.position.x *= 0.7;
        v.position.y = v.position.y * 0.6 + 0.2;
        v.position.z *= 1.5;
    }
    // Deliberately open, reverse-facing mask sheet. Unlike a closed box in
    // empty space, this creates a non-vacuous stencil-one region before bodies.
    // The opposite winding is exercised separately by the fragment oracle.
    mask.indices = vec![4, 5, 6, 6, 7, 4];
    let mut final_mesh = cube();
    for v in &mut final_mesh.vertices {
        v.position.x *= 0.5;
        v.position.y = v.position.y * 0.5 + 0.25;
        v.position.z *= 1.7;
    }
    document.models.push(WorldModel {
        effective_source: AssetKey([27; 32]),
        effective_content: document.revision.content,
        context: ModelContext::Standalone,
        format_version: 3,
        reconstruction_version: 0,
        groups: vec![
            vec![ModelPart {
                texture: 0,
                mesh: body,
            }],
            vec![],
            vec![ModelPart {
                texture: 1,
                mesh: final_mesh,
            }],
        ],
        textures: [[220, 20, 20, 255], [20, 20, 220, 255]]
            .into_iter()
            .enumerate()
            .map(|(i, color)| ModelTexture {
                selector: ModelTextureSelector::Custom { id: i as u16 + 1 },
                effective_asset: AssetKey([28 + i as u8; 32]),
                uv_scale: Vec2::new(1., 1.),
                image: RgbaImage {
                    width: 1,
                    height: 1,
                    pixels: vec![color],
                },
            })
            .collect(),
        bounds: Aabb::new(Vec3::new(-0.4, 0., -0.7), Vec3::new(0.4, 1., 0.7)).unwrap(),
        depth_mask: Some(ModelDepthMask { kind, mesh: mask }),
    });
    bind_content(&mut document);
    document
}

/// Synthetic pack identities include the actual material bytes, not only the
/// empty blueprint shared by several intentionally different fixtures.
pub fn bind_content(document: &mut WorldDocument) {
    use sha2::{Digest, Sha256};
    let mut models = document.models.clone();
    for model in &mut models {
        model.effective_content = AssetKey([0; 32]);
    }
    let bytes = serde_json::to_vec(&(
        "source-material-fixture-v2",
        &document.provenance.origin,
        &document.lot,
        &models,
        &document.materials,
    ))
    .unwrap();
    let content = AssetKey(Sha256::digest(bytes).into());
    document.revision.content = content;
    for model in &mut document.models {
        model.effective_content = content;
    }
}
