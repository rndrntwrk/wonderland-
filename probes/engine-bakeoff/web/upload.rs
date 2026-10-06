//! Exact shared upload preparation: core model transforms are applied once.
use super::{bridge::State, math::id_color};
use wonderland_render_core::{Mat4, RenderLimits, RgbaImage};

pub struct Upload {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub image: RgbaImage,
    /// Red is raw source depth; green is effective alpha (mask replaces alpha).
    pub depth_alpha: RgbaImage,
    pub color: [f32; 4],
    pub light_cutoff: [f32; 4],
    /// back, front, sprite flag, room
    pub sprite: [f32; 4],
    pub id_color: [f32; 4],
    pub unlit: bool,
}
pub fn prepare(state: &State) -> Result<Vec<Upload>, String> {
    let limits = RenderLimits::default();
    let mut uploads = Vec::new();
    for draw in &state.scene.draws {
        if !draw.material.color.iter().all(|v| v.is_finite())
            || draw
                .material
                .alpha_cutoff
                .is_some_and(|a| !a.is_finite() || !(0.0..=1.0).contains(&a))
        {
            return Err("invalid material color/alpha cutoff".into());
        }
        if !draw.material.unlit {
            return Err("fixture comparison currently requires unlit mesh materials".into());
        }
        if !draw.material.double_sided {
            return Err("fixture comparison currently requires double-sided mesh materials".into());
        }
        draw.mesh
            .validate(&limits)
            .map_err(|e| format!("{}: {e:?}", draw.name))?;
        let normal_matrix = draw
            .model
            .inverse()
            .ok_or("singular model matrix")?
            .transpose();
        let image = draw
            .material
            .image
            .clone()
            .unwrap_or_else(|| solid([255; 4]));
        image
            .validate(&limits)
            .map_err(|e| format!("{}: {e:?}", draw.name))?;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut colors = Vec::new();
        for v in &draw.mesh.vertices {
            let p = draw.model.transform_point3(v.position);
            let n = normal_matrix
                .transform_vector3(v.normal)
                .normalize_or_zero();
            if !p.is_finite() || !n.is_finite() {
                return Err("model transform overflow".into());
            }
            positions.push([p.x, p.y, p.z]);
            normals.push([n.x, n.y, n.z]);
            uvs.push([v.uv.x, v.uv.y]);
            colors.push(v.color);
        }
        uploads.push(Upload {
            name: draw.name.clone(),
            positions,
            normals,
            uvs,
            colors,
            indices: draw.mesh.indices.clone(),
            image,
            depth_alpha: solid([255; 4]),
            color: draw.material.color,
            light_cutoff: [1.0, 1.0, 1.0, draw.material.alpha_cutoff.unwrap_or(0.0)],
            sprite: [0.0; 4],
            id_color: id_color(state.pick_index(draw.owner)),
            unlit: draw.material.unlit,
        });
    }
    for sprite in &state.scene.sprites {
        if sprite.lighting.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("invalid sprite lighting".into());
        }
        sprite
            .image
            .validate(&limits)
            .map_err(|e| format!("{}: {e:?}", sprite.name))?;
        let len = sprite.image.pixels.len();
        if sprite.depth.len() != len || sprite.mask.as_ref().is_some_and(|m| m.len() != len) {
            return Err("sprite depth/mask dimensions disagree".into());
        }
        let [x, y, w, h] = sprite.rect;
        if ![x, y, w, h, sprite.back_depth, sprite.front_depth]
            .iter()
            .all(|v| v.is_finite())
            || w <= 0.0
            || h <= 0.0
        {
            return Err("invalid sprite screen rectangle".into());
        }
        let left = x / 320.0 - 1.0;
        let right = (x + w) / 320.0 - 1.0;
        let top = 1.0 - y / 240.0;
        let bottom = 1.0 - (y + h) / 240.0;
        let (u0, u1) = if sprite.mirror {
            (1.0, 0.0)
        } else {
            (0.0, 1.0)
        };
        let pixels = sprite
            .depth
            .iter()
            .enumerate()
            .map(|(i, q)| {
                [
                    *q,
                    sprite
                        .mask
                        .as_ref()
                        .map_or(sprite.image.pixels[i][3], |m| m[i]),
                    0,
                    255,
                ]
            })
            .collect();
        uploads.push(Upload {
            name: sprite.name.clone(),
            positions: vec![
                [left, top, 0.5],
                [left, bottom, 0.5],
                [right, bottom, 0.5],
                [right, top, 0.5],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[u0, 0.0], [u0, 1.0], [u1, 1.0], [u1, 0.0]],
            colors: vec![[1.0; 4]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            image: sprite.image.clone(),
            depth_alpha: RgbaImage {
                width: sprite.image.width,
                height: sprite.image.height,
                pixels,
            },
            color: [1.0; 4],
            light_cutoff: [
                sprite.lighting[0],
                sprite.lighting[1],
                sprite.lighting[2],
                0.0,
            ],
            sprite: [
                sprite.back_depth,
                sprite.front_depth,
                1.0,
                sprite.room as f32,
            ],
            id_color: id_color(state.pick_index(Some(sprite.owner))),
            unlit: true,
        });
    }
    Ok(uploads)
}
pub fn projection(state: &State) -> Result<Mat4, String> {
    state
        .scene
        .camera
        .view_projection(640.0 / 480.0)
        .map_err(|e| e.to_string())
}
fn solid(pixel: [u8; 4]) -> RgbaImage {
    RgbaImage {
        width: 1,
        height: 1,
        pixels: vec![pixel],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::Config;
    #[test]
    fn model_translation_occurs_once_and_preserves_indices() {
        let mut state = State::new(Config::default()).unwrap();
        let vertex = state.scene.draws[0].mesh.vertices[0].position;
        state.scene.draws[0].model =
            Mat4::from_translation(wonderland_render_core::Vec3::new(1.0, 2.0, 3.0));
        let output = prepare(&state).unwrap();
        assert_eq!(
            output[0].positions[0],
            [vertex.x + 1.0, vertex.y + 2.0, vertex.z + 3.0]
        );
        assert_eq!(output[0].indices, state.scene.draws[0].mesh.indices);
    }
    #[test]
    fn sprite_mask_replaces_alpha_and_uses_own_raw_depth() {
        let mut state = State::new(Config::default()).unwrap();
        let sprite = &mut state.scene.sprites[0];
        sprite.image.pixels[0][3] = 255;
        sprite.mask = Some(vec![2; sprite.image.pixels.len()]);
        sprite.depth[0] = 153;
        let output = prepare(&state).unwrap();
        let sprite_output = &output[state.scene.draws.len()];
        assert_eq!(sprite_output.depth_alpha.pixels[0], [153, 2, 0, 255]);
    }
    #[test]
    fn invalid_material_cannot_reach_upload() {
        let mut state = State::new(Config::default()).unwrap();
        state.scene.draws[0].material.color[0] = f32::NAN;
        assert!(prepare(&state).is_err());
    }
    #[test]
    fn single_sided_material_is_rejected_before_an_upload_can_replace_the_scene() {
        let mut state = State::new(Config::default()).unwrap();
        assert!(prepare(&state).is_ok());
        state.scene.draws[0].material.double_sided = false;
        assert_eq!(
            prepare(&state).err().as_deref(),
            Some("fixture comparison currently requires double-sided mesh materials")
        );
    }
}
