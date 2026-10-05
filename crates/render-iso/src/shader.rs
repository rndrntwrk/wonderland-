//! Explicit engine-facing data ABI. No WGSL/GLSL compiler or GPU gate has run.
use crate::*;
use wonderland_render_core::AssetKey;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteShaderVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub world_anchor: [f32; 3],
    pub object_id_and_floor: [f32; 2],
    pub room: [f32; 2],
}
impl SpriteShaderVertex {
    /// Source-compatible 48-byte vertex, explicitly packed as 12 f32 values.
    /// Adapters may upload `to_le_bytes`; no Rust layout or unsafe cast is assumed.
    pub fn floats(self) -> [f32; 12] {
        [
            self.position[0],
            self.position[1],
            self.position[2],
            self.uv[0],
            self.uv[1],
            self.world_anchor[0],
            self.world_anchor[1],
            self.world_anchor[2],
            self.object_id_and_floor[0],
            self.object_id_and_floor[1],
            self.room[0],
            self.room[1],
        ]
    }
}
impl PreparedSprite {
    /// Pick code is allocated by the engine's generation-aware core pick table.
    /// It is not the game's u32 object identity narrowed into a source short.
    pub fn shader_vertices(&self, pick_code: u16) -> Result<[SpriteShaderVertex; 4]> {
        if pick_code == 0 || pick_code > 32767 || self.mesh.vertices.len() != 4 {
            return Err(IsoError::Invalid("shader pick code or quad"));
        }
        let vertex = |i: usize| {
            let v = self.mesh.vertices[i];
            SpriteShaderVertex {
                position: [v.position.x, v.position.y, v.position.z],
                uv: [v.uv.x, v.uv.y],
                world_anchor: [
                    self.world_anchor.x,
                    self.world_anchor.y,
                    self.world_anchor.z,
                ],
                object_id_and_floor: [f32::from(pick_code) / 65535., f32::from(self.floor_index)],
                room: [self.room_uv.x, self.room_uv.y],
            }
        };
        let vertices = [vertex(0), vertex(1), vertex(2), vertex(3)];
        if vertices
            .iter()
            .flat_map(|v| v.floats())
            .any(|x| !x.is_finite())
        {
            return Err(IsoError::Invalid("shader vertex arithmetic"));
        }
        Ok(vertices)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    PointClamp,
    LinearClamp,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightBinding {
    Texture(AssetKey),
    Constant([f32; 4]),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightingBindings {
    pub ambient: LightBinding,
    pub advanced: LightBinding,
    pub direction: LightBinding,
    pub ambient_sampling: Sampling,
    pub advanced_sampling: Sampling,
    pub direction_sampling: Sampling,
}
pub fn lighting_bindings(
    resources: LightingResources,
    outside_pixel: [f32; 4],
) -> Result<LightingBindings> {
    if outside_pixel
        .iter()
        .any(|x| !x.is_finite() || !(0. ..=1.).contains(x))
    {
        return Err(IsoError::Invalid("outside fallback pixel"));
    }
    let binding = |resource: Option<AssetKey>, fallback: [f32; 4]| {
        resource.map_or(LightBinding::Constant(fallback), LightBinding::Texture)
    };
    Ok(LightingBindings {
        ambient: binding(resources.ambient, [1.; 4]),
        advanced: binding(resources.advanced, outside_pixel),
        direction: binding(resources.direction, [128. / 255., 0., 0., 1.]),
        ambient_sampling: Sampling::PointClamp,
        advanced_sampling: Sampling::LinearClamp,
        direction_sampling: Sampling::LinearClamp,
    })
}
/// CPU comparison of the nominal XNA blend factors. Source's dynamic path uses
/// NonPremultiplied after its shader has already premultiplied RGB. Retained as
/// an opt-in comparison; the coherent target default is premultiplied blending.
pub fn blend_over(
    source: [f32; 4],
    destination: [f32; 4],
    policy: BlendPolicy,
) -> Result<[f32; 4]> {
    if source
        .iter()
        .chain(destination.iter())
        .any(|v| !v.is_finite())
        || !(0. ..=1.).contains(&source[3])
        || !(0. ..=1.).contains(&destination[3])
    {
        return Err(IsoError::Invalid("blend input"));
    }
    let factor = match policy {
        BlendPolicy::Premultiplied => 1.,
        BlendPolicy::LegacyDynamicNonPremultiplied => source[3],
    };
    let mut color = [0.; 4];
    for i in 0..4 {
        color[i] = source[i] * factor + destination[i] * (1. - source[3]);
    }
    if color.iter().any(|v| !v.is_finite()) {
        return Err(IsoError::Invalid("blend arithmetic overflow"));
    }
    Ok(color)
}
