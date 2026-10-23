//! Exact retained FreeSO banner visual intake. No reconstruction, replacement
//! texture, download or altered source geometry is introduced.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wonderland_content_ir::manifest::Digest;
use wonderland_legacy_formats::{
    Limits, iff,
    reconstruction::{FsomGeometry, FsomTextureReference, decode_fsom},
    semantic::decode_objd,
    sprites::decode_dgrp,
    textures::decode_png,
};
use wonderland_render_core::{Aabb, AssetKey, Mesh, RgbaImage, Vec2, Vec3, Vertex};
use wonderland_world_view::{
    ModelContext, ModelPart, ModelTexture, ModelTextureSelector, WorldModel,
};

pub const GUID: u32 = 0x6378e9e6;
pub const SOURCE_SHA256: &str = "260a9db1ed017eacf2604beaf92dbf0cafae6c651b1f0296605eab2429503ab7";
pub const SOURCE_PATH: &str =
    "TSOClient/FSO.Content.TSO/Content/Objects/fso_decor_2-in-1_Racing_Banner.iff";

fn asset(bytes: &[u8]) -> AssetKey {
    let digest = Digest::of(bytes);
    let mut result = [0; 32];
    for (out, pair) in result
        .iter_mut()
        .zip(digest.as_str().as_bytes().chunks_exact(2))
    {
        *out = u8::from_str_radix(std::str::from_utf8(pair).expect("digest ASCII"), 16)
            .expect("digest hex");
    }
    AssetKey(result)
}
fn mesh(source: &FsomGeometry) -> Mesh {
    Mesh {
        vertices: source
            .vertices
            .iter()
            .map(|vertex| Vertex {
                position: Vec3::new(
                    vertex.position[0].get(),
                    vertex.position[1].get(),
                    vertex.position[2].get(),
                ),
                uv: Vec2::new(
                    vertex.texture_coordinate[0].get(),
                    vertex.texture_coordinate[1].get(),
                ),
                normal: Vec3::new(
                    vertex.normal[0].get(),
                    vertex.normal[1].get(),
                    vertex.normal[2].get(),
                ),
                color: [1.; 4],
            })
            .collect(),
        indices: source.indices.iter().map(|index| *index as u32).collect(),
    }
}

pub fn load(
    bytes: &[u8],
    content: [u8; 32],
) -> Result<(Vec<WorldModel>, Value), Box<dyn std::error::Error>> {
    if Digest::of(bytes).as_str() != SOURCE_SHA256 {
        return Err("banner differs from retained original source pin".into());
    }
    let limits = Limits::default();
    let file = iff::decode(bytes, &limits)?;
    let object = file
        .chunks
        .iter()
        .find(|chunk| chunk.key.kind == *b"OBJD")
        .ok_or("banner OBJD missing")?;
    let object = decode_objd(&object.data, &limits)?;
    if object.guid() != GUID
        || object.field("BaseGraphicID") != Some(100)
        || object.field("NumGraphics") != Some(6)
    {
        return Err("unexpected original banner definition".into());
    }
    let mut textures = BTreeMap::new();
    let mut texture_receipts = Vec::new();
    for chunk in file
        .chunks
        .iter()
        .filter(|chunk| chunk.key.kind == *b"MTEX")
    {
        let image = decode_png(&chunk.data, &limits)?;
        texture_receipts.push(json!({"kind":"MTEX","id":chunk.key.id,"sha256":Digest::of(&chunk.data),"width":image.width,"height":image.height}));
        textures.insert(
            chunk.key.id,
            ModelTexture {
                selector: ModelTextureSelector::Custom { id: chunk.key.id },
                effective_asset: asset(&chunk.data),
                uv_scale: Vec2::new(1., 1.),
                image: RgbaImage {
                    width: image.width,
                    height: image.height,
                    pixels: image.rgba,
                },
            },
        );
    }
    let mut models = Vec::new();
    let mut mesh_receipts = Vec::new();
    for id in 100..=105 {
        let drawing = file
            .chunks
            .iter()
            .find(|chunk| chunk.key.kind == *b"DGRP" && chunk.key.id == id)
            .ok_or("original banner DGRP missing")?;
        decode_dgrp(&drawing.data, &limits)?;
        let chunk = file
            .chunks
            .iter()
            .find(|chunk| chunk.key.kind == *b"FSOM" && chunk.key.id == id)
            .ok_or("original banner FSOM missing")?;
        let source = decode_fsom(&chunk.data, &limits)?;
        if source.depth_mask.is_some() || source.mask_type != 0 {
            return Err("unexpected banner depth-mask dependency".into());
        }
        let mut model_textures = Vec::new();
        let mut indices = BTreeMap::new();
        let mut groups = Vec::new();
        for group in &source.groups {
            let mut parts = Vec::new();
            for geometry in group {
                let FsomTextureReference::CustomTexture { id } = geometry.texture_reference()
                else {
                    return Err("banner requires unqualified sprite texture path".into());
                };
                let texture = if let Some(index) = indices.get(&id) {
                    *index
                } else {
                    let index = model_textures.len();
                    model_textures.push(
                        textures
                            .get(&id)
                            .ok_or("same-IFF original MTEX missing")?
                            .clone(),
                    );
                    indices.insert(id, index);
                    index
                };
                parts.push(ModelPart {
                    texture,
                    mesh: mesh(geometry),
                });
            }
            groups.push(parts);
        }
        let vector = |v: &[wonderland_legacy_formats::vitaboy::F32Bits; 3]| {
            Vec3::new(v[0].get(), v[1].get(), v[2].get())
        };
        models.push(WorldModel {
            effective_source: asset(&chunk.data),
            effective_content: AssetKey(content),
            context: ModelContext::Dgrp {
                effective_iff: asset(bytes),
                chunk_id: id,
            },
            format_version: source.version.try_into()?,
            reconstruction_version: source.reconstruct_version.try_into()?,
            groups,
            textures: model_textures,
            bounds: Aabb::new(vector(&source.bounds[0]), vector(&source.bounds[1]))
                .ok_or("invalid original model bounds")?,
            depth_mask: None,
        });
        mesh_receipts.push(json!({"dgrp_id":id,"dgrp_sha256":Digest::of(&drawing.data),"fsom_sha256":Digest::of(&chunk.data),"source_name":source.name,"format_version":source.version,"reconstruction_version":source.reconstruct_version,"custom_texture_ids":indices.keys().copied().collect::<Vec<_>>()}));
    }
    Ok((
        models,
        json!({"source_path":SOURCE_PATH,"source_sha256":SOURCE_SHA256,"guid":GUID,"models":mesh_receipts,"textures":texture_receipts,"geometry_altered":false,"replacement_texture_provider":"explicitly absent; same-IFF MTEX fallback","sprite_archive_required":false}),
    ))
}
