//! Normalized FSOm admission and ordered object draws. B owns byte decoding and
//! provider lookup; C owns validation, disposable meshes, materials and passes.

use crate::reconstruction::MaskType;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use wonderland_render_core::{
    Aabb, AssetKey, EntityRef, Mat4, Mesh, RenderLimits, RgbaImage, Vec2, Vec3, Vertex,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectError {
    Invalid(&'static str),
    Limit(&'static str),
    Unsupported(&'static str),
    StaleRequest,
    GenerationExhausted,
}
impl std::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ObjectError {}

/// Hashes of the selected, patched FSOm bytes and the effective content set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectMeshIdentity {
    pub effective_source: AssetKey,
    pub effective_content: AssetKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsomContext {
    Dgrp {
        effective_iff: AssetKey,
        chunk_id: u16,
    },
    Standalone,
}

/// `PixelSPR` is an ordinal in GetImage(1, 3, rotation), not a sprite ID.
/// Custom is the source's PixelDir=65535 path (replacement PNG, then MTEX).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextureSelector {
    Sprite { rotation: u16, ordinal: u16 },
    Custom { id: u16 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedTexture {
    pub selector: TextureSelector,
    pub effective_asset: AssetKey,
    /// Original image size / allocated texture size. UVs remain unscaled in Mesh.
    pub uv_scale: Vec2,
    /// Decoded, straight-alpha pixels in the allocated texture dimensions.
    pub image: RgbaImage,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FsomVertex {
    pub position: Vec3,
    pub uv: Vec2,
    /// Ignored for v1, which generates the source's area-weighted normals.
    pub normal: Vec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizedGeometry {
    pub vertices: Vec<FsomVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedPart {
    /// Index into NormalizedFsom::textures, preserving the resolved binding.
    pub texture: usize,
    pub geometry: NormalizedGeometry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedDepthMask {
    /// Must be Normal or Portal. A mask never samples an object texture.
    pub kind: MaskType,
    pub geometry: NormalizedGeometry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedFsom {
    pub identity: ObjectMeshIdentity,
    pub context: FsomContext,
    pub format_version: u32,
    pub reconstruction_version: u32,
    /// File order, including empty groups: 0 is static, 1..128 are dynamic.
    pub groups: Vec<Vec<NormalizedPart>>,
    pub textures: Vec<NormalizedTexture>,
    /// Source object bounds in local tile-sized, Y-up mesh coordinates. Portal
    /// mask geometry may extend outside these bounds, as in the original source.
    pub bounds: Aabb,
    pub depth_mask: Option<NormalizedDepthMask>,
}

#[derive(Clone, Copy, Debug)]
pub struct ObjectLimits {
    /// Vertex/index limits apply to the entire object, including its depth mask.
    pub render: RenderLimits,
    pub max_parts: usize,
    pub max_textures: usize,
    pub max_total_texture_pixels: usize,
    /// Retained vertex, index and pixel buffers; metadata is bounded separately.
    pub max_buffer_bytes: usize,
}
impl Default for ObjectLimits {
    fn default() -> Self {
        Self {
            render: RenderLimits::default(),
            max_parts: 4096,
            max_textures: 4096,
            max_total_texture_pixels: 16_777_216,
            max_buffer_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreparedTexture {
    source: NormalizedTexture,
}
impl PreparedTexture {
    pub fn selector(&self) -> TextureSelector {
        self.source.selector
    }
    pub fn effective_asset(&self) -> AssetKey {
        self.source.effective_asset
    }
    pub fn uv_scale(&self) -> Vec2 {
        self.source.uv_scale
    }
    pub fn image(&self) -> &RgbaImage {
        &self.source.image
    }
    /// RCObject's TexSampler: linear min/mag/mip filtering, clamp U/V.
    pub fn sampling(&self) -> TextureSampling {
        TextureSampling::LinearClampMipmaps
    }
    pub fn shader_uv(&self, uv: Vec2) -> Result<Vec2, ObjectError> {
        if !uv.is_finite() {
            return Err(ObjectError::Invalid("shader UV"));
        }
        Ok(Vec2::new(
            uv.x * self.source.uv_scale.x,
            uv.y * self.source.uv_scale.y,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureSampling {
    LinearClampMipmaps,
}

#[derive(Clone, Debug)]
pub struct PreparedPart {
    texture: usize,
    mesh: Mesh,
}
impl PreparedPart {
    pub fn texture_index(&self) -> usize {
        self.texture
    }
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }
}

#[derive(Clone, Debug)]
pub struct PreparedFsom {
    identity: ObjectMeshIdentity,
    context: FsomContext,
    format_version: u32,
    reconstruction_version: u32,
    key: AssetKey,
    bounds: Aabb,
    draw_bounds: Aabb,
    normal_bounds: Option<Aabb>,
    groups: Vec<Vec<PreparedPart>>,
    textures: Vec<PreparedTexture>,
    depth_mask: Option<(MaskType, Mesh)>,
    buffer_bytes: usize,
}
impl PreparedFsom {
    pub fn prepare(source: NormalizedFsom, limits: ObjectLimits) -> Result<Self, ObjectError> {
        if !(1..=3).contains(&source.format_version) {
            return Err(ObjectError::Unsupported("FSOM format version"));
        }
        if source.reconstruction_version != 0 && source.reconstruction_version < 2 {
            return Err(ObjectError::Unsupported("obsolete reconstruction version"));
        }
        // The source field is signed Int32; an unsigned normalization must not
        // admit a wrapped negative value as a future reconstruction version.
        if source.reconstruction_version > i32::MAX as u32 {
            return Err(ObjectError::Unsupported("reconstruction version range"));
        }
        if source.groups.len() > 129 {
            return Err(ObjectError::Limit("dynamic groups"));
        }
        if source.textures.len() > limits.max_textures {
            return Err(ObjectError::Limit("object textures"));
        }
        if Aabb::new(source.bounds.min, source.bounds.max).is_none() {
            return Err(ObjectError::Invalid("source bounds"));
        }
        if let Some(mask) = &source.depth_mask {
            if source.format_version < 3 || mask.kind == MaskType::None {
                return Err(ObjectError::Invalid("depth mask version/type"));
            }
        }

        // Admission accounts for the whole asset before allocating output meshes.
        let (mut vertices, mut indices, mut parts, mut pixels) = (0usize, 0usize, 0usize, 0usize);
        let mut selectors = BTreeSet::new();
        for texture in &source.textures {
            if !selectors.insert(texture.selector) {
                return Err(ObjectError::Invalid("duplicate texture selector"));
            }
            if let TextureSelector::Sprite { rotation, .. } = texture.selector {
                if rotation > 3 || matches!(source.context, FsomContext::Standalone) {
                    return Err(ObjectError::Invalid("DGRP sprite texture selector"));
                }
            }
            if !texture.uv_scale.is_finite()
                || texture.uv_scale.x <= 0.
                || texture.uv_scale.y <= 0.
                || texture.uv_scale.x > 1.
                || texture.uv_scale.y > 1.
            {
                return Err(ObjectError::Invalid("texture UV scale"));
            }
            texture
                .image
                .validate(&limits.render)
                .map_err(|_| ObjectError::Invalid("texture image"))?;
            pixels = add_limit(
                pixels,
                texture.image.pixels.len(),
                limits.max_total_texture_pixels,
                "object texture pixels",
            )?;
        }
        for group in &source.groups {
            parts = add_limit(parts, group.len(), limits.max_parts, "object parts")?;
            let mut bindings = BTreeSet::new();
            for part in group {
                if part.texture >= source.textures.len() {
                    return Err(ObjectError::Invalid("texture binding"));
                }
                if !bindings.insert(part.texture) {
                    return Err(ObjectError::Invalid("duplicate texture within group"));
                }
                check_geometry(
                    &part.geometry,
                    Some(source.bounds),
                    &limits,
                    &mut vertices,
                    &mut indices,
                )?;
            }
        }
        if let Some(mask) = &source.depth_mask {
            check_geometry(&mask.geometry, None, &limits, &mut vertices, &mut indices)?;
        }
        let bytes = vertices
            .checked_mul(std::mem::size_of::<Vertex>())
            .and_then(|v| indices.checked_mul(4).and_then(|i| v.checked_add(i)))
            .and_then(|v| pixels.checked_mul(4).and_then(|p| v.checked_add(p)))
            .ok_or(ObjectError::Limit("object buffers"))?;
        if bytes > limits.max_buffer_bytes {
            return Err(ObjectError::Limit("object buffers"));
        }
        let mut groups = Vec::with_capacity(source.groups.len());
        for group in source.groups {
            let mut prepared = Vec::with_capacity(group.len());
            for part in group {
                prepared.push(PreparedPart {
                    texture: part.texture,
                    mesh: prepare_geometry(part.geometry, source.format_version)?,
                });
            }
            groups.push(prepared);
        }
        let depth_mask = source
            .depth_mask
            .map(|mask| {
                prepare_geometry(mask.geometry, source.format_version).map(|mesh| (mask.kind, mesh))
            })
            .transpose()?;
        let mut draw_bounds = source.bounds;
        let mut normal_bounds = None;
        for mesh in groups
            .iter()
            .flatten()
            .map(|p| &p.mesh)
            .chain(depth_mask.iter().map(|(_, mesh)| mesh))
        {
            for vertex in &mesh.vertices {
                include_point(&mut normal_bounds, vertex.normal);
            }
        }
        if let Some((_, mask)) = &depth_mask {
            let mut bounds = Some(draw_bounds);
            for vertex in &mask.vertices {
                include_point(&mut bounds, vertex.position);
            }
            draw_bounds = bounds.unwrap();
        }
        let mut prepared = Self {
            identity: source.identity,
            context: source.context,
            format_version: source.format_version,
            reconstruction_version: source.reconstruction_version,
            key: AssetKey([0; 32]),
            bounds: source.bounds,
            draw_bounds,
            normal_bounds,
            groups,
            textures: source
                .textures
                .into_iter()
                .map(|source| PreparedTexture { source })
                .collect(),
            depth_mask,
            buffer_bytes: bytes,
        };
        prepared.key = content_key(&prepared);
        Ok(prepared)
    }
    pub fn identity(&self) -> ObjectMeshIdentity {
        self.identity
    }
    pub fn context(&self) -> FsomContext {
        self.context
    }
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    pub fn reconstruction_version(&self) -> u32 {
        self.reconstruction_version
    }
    pub fn key(&self) -> AssetKey {
        self.key
    }
    pub fn bounds(&self) -> Aabb {
        self.bounds
    }
    pub fn groups(&self) -> &[Vec<PreparedPart>] {
        &self.groups
    }
    pub fn textures(&self) -> &[PreparedTexture] {
        &self.textures
    }
    pub fn depth_mask(&self) -> Option<(MaskType, &Mesh)> {
        self.depth_mask.as_ref().map(|(kind, mesh)| (*kind, mesh))
    }
    pub fn buffer_bytes(&self) -> usize {
        self.buffer_bytes
    }

    pub fn scene(
        &self,
        instance: ObjectInstance,
        target: ObjectTarget,
    ) -> Result<ObjectScene<'_>, ObjectError> {
        check_entity(instance.entity)?;
        if !instance.position_tiles.is_finite() || !instance.yaw_radians.is_finite() {
            return Err(ObjectError::Invalid("object placement"));
        }
        let position = wonderland_render_core::units::tile_to_graphics(instance.position_tiles)
            + Vec3::new(1.5, 0.1, 1.5);
        // XNA row-vector Scale*RotationY(-direction)*Translation, transposed to
        // core's column-vector convention. Mesh coordinates are already Y-up.
        let angle = -f64::from(instance.yaw_radians);
        let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
        let rotation = Mat4 {
            cols: [
                [cos, 0., -sin, 0.],
                [0., 1., 0., 0.],
                [sin, 0., cos, 0.],
                [0., 0., 0., 1.],
            ],
        };
        let mut world =
            Mat4::from_translation(position) * rotation * Mat4::from_scale(Vec3::new(3., 3., 3.));
        if let ObjectTarget::Lightmap { level, y_offset } = target {
            if !y_offset.is_finite() {
                return Err(ObjectError::Invalid("lightmap offset"));
            }
            // DrawLMap overwrites M42 with this exact equation, without another *3.
            world.cols[3][1] =
                ((i16::from(instance.level) - i16::from(level)) - 1) as f32 * 2.95 + y_offset;
        }
        let bounds = self
            .bounds
            .transformed(world)
            .ok_or(ObjectError::Invalid("object world bounds"))?;
        let render_bounds = self
            .draw_bounds
            .transformed(world)
            .ok_or(ObjectError::Invalid("mask world bounds"))?;
        let mut normal_matrix = world;
        normal_matrix.cols[3] = [0., 0., 0., 1.];
        if self
            .normal_bounds
            .map(|b| b.transformed(normal_matrix).is_none())
            .unwrap_or(false)
        {
            return Err(ObjectError::Invalid("object world normals"));
        }
        let mask_kind = self
            .depth_mask
            .as_ref()
            .map(|(kind, _)| *kind)
            .unwrap_or(MaskType::None);
        let lightmap = matches!(target, ObjectTarget::Lightmap { .. });
        let shader = if lightmap {
            ObjectShader::Lightmap
        } else if instance.room == 65533 {
            ObjectShader::Disabled
        } else if instance.directional_lighting && instance.room < 65533 {
            ObjectShader::Directional
        } else {
            ObjectShader::Basic
        };
        let mut draws = Vec::new();
        if !(lightmap && mask_kind == MaskType::Portal) {
            let mask = self
                .depth_mask
                .as_ref()
                .map(|(_, mesh)| mesh)
                .filter(|mesh| !mesh.indices.is_empty());
            if !lightmap {
                if let Some(mesh) = mask {
                    draws.push(mask_draw(mesh, ObjectShader::MaskMark, mark_pipeline()));
                    draws.push(mask_draw(
                        mesh,
                        ObjectShader::MaskFar,
                        clear_pipeline(mask_kind == MaskType::Portal, false),
                    ));
                }
            }
            for (group_index, group) in self.groups.iter().enumerate() {
                let portal_final =
                    mask_kind == MaskType::Portal && group_index + 1 == self.groups.len();
                let visible = group_index == 0
                    || portal_final
                    || instance.dynamic_flags[(group_index - 1) / 64]
                        & (1u64 << ((group_index - 1) % 64))
                        != 0;
                if !visible {
                    continue;
                }
                for (part_index, part) in group.iter().enumerate() {
                    if part.mesh.indices.is_empty() {
                        continue;
                    }
                    let mut pipeline = body_pipeline(if lightmap {
                        ObjectBlend::MaxGreen
                    } else {
                        ObjectBlend::NonPremultiplied
                    });
                    if lightmap {
                        // LMapBatch's object and outside shadow targets use
                        // DepthFormat.None: overlapping fragments all reach Max.
                        pipeline.depth_compare = DepthComparison::Always;
                        pipeline.depth_write = false;
                    }
                    if portal_final {
                        pipeline.stencil = Some(equal_stencil(
                            StencilOperation::Keep,
                            StencilOperation::Keep,
                        ));
                    }
                    draws.push(ObjectDraw {
                        group_part: Some((group_index as u16, part_index)),
                        mesh: &part.mesh,
                        material: if lightmap {
                            None
                        } else {
                            Some(&self.textures[part.texture])
                        },
                        shader,
                        pipeline,
                    });
                }
            }
            if !lightmap && mask_kind == MaskType::Portal {
                if let Some(mesh) = mask {
                    draws.push(mask_draw(
                        mesh,
                        ObjectShader::MaskFar,
                        clear_pipeline(false, true),
                    ));
                }
            }
        }
        Ok(ObjectScene {
            identity: self.identity,
            asset_key: self.key,
            entity: instance.entity,
            visual_revision: instance.visual_revision,
            world,
            bounds,
            render_bounds,
            room: instance.room,
            shader_level: f32::from(instance.level) - 0.999,
            draws,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectInstance {
    pub entity: EntityRef,
    pub visual_revision: u64,
    /// Simulation projection in tile coordinates (X/Y horizontal, Z elevation).
    pub position_tiles: Vec3,
    pub yaw_radians: f32,
    pub dynamic_flags: [u64; 2],
    pub room: u16,
    pub level: i8,
    /// True only when both original advanced and directional lighting are enabled.
    pub directional_lighting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectTarget {
    Color,
    Lightmap { level: i8, y_offset: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectShader {
    Basic,
    Directional,
    Disabled,
    MaskMark,
    MaskFar,
    Lightmap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthComparison {
    LessEqual,
    Always,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StencilComparison {
    Always,
    Equal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StencilOperation {
    Keep,
    Zero,
    Replace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StencilFace {
    pub compare: StencilComparison,
    pub pass: StencilOperation,
    pub fail: StencilOperation,
    pub depth_fail: StencilOperation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StencilState {
    pub reference: u8,
    /// Winding is the source XNA convention after projection. Engines must map
    /// front/back to these explicit clockwise/counterclockwise faces.
    pub clockwise: StencilFace,
    pub counterclockwise: StencilFace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectBlend {
    /// SourceAlpha / InverseSourceAlpha, Add, for BOTH RGB and alpha, matching
    /// XNA BlendState.NonPremultiplied. This is not the usual alpha-over equation.
    NonPremultiplied,
    /// Source LMapBatch MaxBlendGreen writes max(source,dest) in green and alpha.
    MaxGreen,
    NoColor,
}
impl ObjectBlend {
    /// Blend already-shaded normalized RGBA samples. Lighting and texture
    /// filtering happen before this step; this oracle does not substitute them.
    pub fn blend_rgba(
        self,
        source: [f32; 4],
        destination: [f32; 4],
    ) -> Result<[f32; 4], ObjectError> {
        if source
            .iter()
            .chain(destination.iter())
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(ObjectError::Invalid("blend color"));
        }
        Ok(match self {
            Self::NonPremultiplied => {
                let mut color = [0.; 4];
                for i in 0..4 {
                    color[i] = source[i] * source[3] + destination[i] * (1. - source[3]);
                }
                color
            }
            Self::MaxGreen => [
                destination[0],
                source[1].max(destination[1]),
                destination[2],
                source[3].max(destination[3]),
            ],
            Self::NoColor => destination,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPipeline {
    pub depth_compare: DepthComparison,
    pub depth_write: bool,
    pub forced_depth: Option<f32>,
    pub stencil: Option<StencilState>,
    pub blend: ObjectBlend,
    /// RC objects use CullNone; the mask needs both winding directions.
    pub cull_faces: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthStencilPixel {
    pub depth: f32,
    pub stencil: u8,
}

#[derive(Clone, Debug)]
pub struct ObjectDraw<'a> {
    /// None for depth-mask operations. Tuple is original group and part ordinal.
    pub group_part: Option<(u16, usize)>,
    pub mesh: &'a Mesh,
    pub material: Option<&'a PreparedTexture>,
    pub shader: ObjectShader,
    pub pipeline: ObjectPipeline,
}
impl ObjectDraw<'_> {
    /// Executable depth/stencil oracle for an already rasterized fragment. Alpha
    /// is the shader's final alpha; this does not approximate texture filtering.
    pub fn apply_depth_stencil(
        &self,
        pixel: &mut DepthStencilPixel,
        depth: f32,
        counterclockwise: bool,
        alpha: f32,
    ) -> Result<bool, ObjectError> {
        let effective_depth = self.pipeline.forced_depth.unwrap_or(depth);
        if [depth, effective_depth, pixel.depth, alpha]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(ObjectError::Invalid("depth/stencil fragment"));
        }
        if matches!(self.shader, ObjectShader::Basic | ObjectShader::Directional) && alpha < 0.01 {
            return Ok(false);
        }
        let face = self.pipeline.stencil.map(|s| {
            (
                if counterclockwise {
                    s.counterclockwise
                } else {
                    s.clockwise
                },
                s.reference,
            )
        });
        if let Some((face, reference)) = face {
            if face.compare == StencilComparison::Equal && pixel.stencil != reference {
                pixel.stencil = stencil_op(face.fail, pixel.stencil, reference);
                return Ok(false);
            }
        }
        if self.pipeline.depth_compare == DepthComparison::LessEqual
            && effective_depth > pixel.depth
        {
            if let Some((face, reference)) = face {
                pixel.stencil = stencil_op(face.depth_fail, pixel.stencil, reference);
            }
            return Ok(false);
        }
        if let Some((face, reference)) = face {
            pixel.stencil = stencil_op(face.pass, pixel.stencil, reference);
        }
        if self.pipeline.depth_write {
            pixel.depth = effective_depth;
        }
        Ok(true)
    }
}

#[derive(Debug)]
pub struct ObjectScene<'a> {
    pub identity: ObjectMeshIdentity,
    pub asset_key: AssetKey,
    pub entity: EntityRef,
    pub visual_revision: u64,
    pub world: Mat4,
    pub bounds: Aabb,
    /// Includes a portal's outlying mask; object selection still uses `bounds`.
    pub render_bounds: Aabb,
    pub room: u16,
    pub shader_level: f32,
    pub draws: Vec<ObjectDraw<'a>>,
}

/// Opaque completion identity. Recreating a slot requires a fresh session
/// generation supplied by its owner; resetting an existing slot fences itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectMeshTicket {
    session_generation: u64,
    reset_generation: u64,
    entity: EntityRef,
    content_generation: u64,
    request_serial: u64,
    identity: ObjectMeshIdentity,
}
impl ObjectMeshTicket {
    pub fn session_generation(self) -> u64 {
        self.session_generation
    }
    pub fn entity(self) -> EntityRef {
        self.entity
    }
    pub fn content_generation(self) -> u64 {
        self.content_generation
    }
    pub fn request_serial(self) -> u64 {
        self.request_serial
    }
    pub fn identity(self) -> ObjectMeshIdentity {
        self.identity
    }
}

#[derive(Clone, Debug)]
pub struct InstalledFsom {
    ticket: ObjectMeshTicket,
    mesh: PreparedFsom,
}
impl InstalledFsom {
    pub fn ticket(&self) -> ObjectMeshTicket {
        self.ticket
    }
    pub fn mesh(&self) -> &PreparedFsom {
        &self.mesh
    }
}

/// One bounded presentation asset slot. A rejected or pending replacement keeps
/// the last complete model and its generation; no A/simulation state is mutated.
pub struct ObjectMeshSlot {
    session_generation: u64,
    reset_generation: u64,
    entity: EntityRef,
    limits: ObjectLimits,
    serial: u64,
    latest_content_generation: Option<u64>,
    pending: Option<ObjectMeshTicket>,
    current: Option<InstalledFsom>,
}
impl ObjectMeshSlot {
    pub fn new(
        session_generation: u64,
        entity: EntityRef,
        limits: ObjectLimits,
    ) -> Result<Self, ObjectError> {
        check_entity(entity)?;
        if session_generation == 0 {
            return Err(ObjectError::Invalid("zero mesh session generation"));
        }
        Ok(Self {
            session_generation,
            reset_generation: 0,
            entity,
            limits,
            serial: 0,
            latest_content_generation: None,
            pending: None,
            current: None,
        })
    }
    pub fn request(
        &mut self,
        identity: ObjectMeshIdentity,
        content_generation: u64,
    ) -> Result<ObjectMeshTicket, ObjectError> {
        if self
            .latest_content_generation
            .map(|g| content_generation < g)
            .unwrap_or(false)
        {
            return Err(ObjectError::StaleRequest);
        }
        let serial = self
            .serial
            .checked_add(1)
            .ok_or(ObjectError::GenerationExhausted)?;
        let ticket = ObjectMeshTicket {
            session_generation: self.session_generation,
            reset_generation: self.reset_generation,
            entity: self.entity,
            content_generation,
            request_serial: serial,
            identity,
        };
        self.serial = serial;
        self.latest_content_generation = Some(content_generation);
        self.pending = Some(ticket);
        Ok(ticket)
    }
    pub fn install(
        &mut self,
        ticket: ObjectMeshTicket,
        source: NormalizedFsom,
    ) -> Result<AssetKey, ObjectError> {
        if self.pending != Some(ticket) || source.identity != ticket.identity {
            return Err(ObjectError::StaleRequest);
        }
        let mesh = PreparedFsom::prepare(source, self.limits)?;
        let key = mesh.key;
        self.current = Some(InstalledFsom { ticket, mesh });
        self.pending = None;
        Ok(key)
    }
    pub fn reset(&mut self, entity: EntityRef) -> Result<(), ObjectError> {
        check_entity(entity)?;
        let generation = self
            .reset_generation
            .checked_add(1)
            .ok_or(ObjectError::GenerationExhausted)?;
        self.reset_generation = generation;
        self.entity = entity;
        self.current = None;
        self.pending = None;
        self.latest_content_generation = None;
        Ok(())
    }
    pub fn current(&self) -> Option<&InstalledFsom> {
        self.current.as_ref()
    }
    pub fn pending(&self) -> Option<ObjectMeshTicket> {
        self.pending
    }
}

fn check_entity(entity: EntityRef) -> Result<(), ObjectError> {
    if entity.object_id == 0 || entity.generation == 0 {
        return Err(ObjectError::Invalid("object entity generation"));
    }
    Ok(())
}
fn include_point(bounds: &mut Option<Aabb>, point: Vec3) {
    *bounds = Some(match *bounds {
        None => Aabb {
            min: point,
            max: point,
        },
        Some(b) => Aabb {
            min: Vec3::new(
                b.min.x.min(point.x),
                b.min.y.min(point.y),
                b.min.z.min(point.z),
            ),
            max: Vec3::new(
                b.max.x.max(point.x),
                b.max.y.max(point.y),
                b.max.z.max(point.z),
            ),
        },
    });
}
fn body_pipeline(blend: ObjectBlend) -> ObjectPipeline {
    ObjectPipeline {
        depth_compare: DepthComparison::LessEqual,
        depth_write: true,
        forced_depth: None,
        stencil: None,
        blend,
        cull_faces: false,
    }
}
fn mark_pipeline() -> ObjectPipeline {
    let clockwise = StencilFace {
        compare: StencilComparison::Always,
        pass: StencilOperation::Zero,
        fail: StencilOperation::Keep,
        depth_fail: StencilOperation::Keep,
    };
    let counterclockwise = StencilFace {
        pass: StencilOperation::Replace,
        ..clockwise
    };
    ObjectPipeline {
        stencil: Some(StencilState {
            reference: 1,
            clockwise,
            counterclockwise,
        }),
        ..body_pipeline(ObjectBlend::NoColor)
    }
}
fn equal_stencil(pass: StencilOperation, depth_fail: StencilOperation) -> StencilState {
    let face = StencilFace {
        compare: StencilComparison::Equal,
        pass,
        fail: StencilOperation::Keep,
        depth_fail,
    };
    StencilState {
        reference: 1,
        clockwise: face,
        counterclockwise: face,
    }
}
fn clear_pipeline(portal: bool, cleanup: bool) -> ObjectPipeline {
    ObjectPipeline {
        depth_compare: DepthComparison::Always,
        depth_write: !cleanup,
        forced_depth: Some(1.),
        stencil: Some(equal_stencil(
            if portal {
                StencilOperation::Keep
            } else {
                StencilOperation::Zero
            },
            if portal || cleanup {
                StencilOperation::Keep
            } else {
                StencilOperation::Zero
            },
        )),
        blend: ObjectBlend::NoColor,
        cull_faces: false,
    }
}
fn mask_draw(mesh: &Mesh, shader: ObjectShader, pipeline: ObjectPipeline) -> ObjectDraw<'_> {
    ObjectDraw {
        group_part: None,
        mesh,
        material: None,
        shader,
        pipeline,
    }
}
fn stencil_op(operation: StencilOperation, current: u8, reference: u8) -> u8 {
    match operation {
        StencilOperation::Keep => current,
        StencilOperation::Zero => 0,
        StencilOperation::Replace => reference,
    }
}

fn add_limit(
    current: usize,
    count: usize,
    limit: usize,
    name: &'static str,
) -> Result<usize, ObjectError> {
    current
        .checked_add(count)
        .filter(|&n| n <= limit)
        .ok_or(ObjectError::Limit(name))
}

fn check_geometry(
    geometry: &NormalizedGeometry,
    body_bounds: Option<Aabb>,
    limits: &ObjectLimits,
    vertices: &mut usize,
    indices: &mut usize,
) -> Result<(), ObjectError> {
    *vertices = add_limit(
        *vertices,
        geometry.vertices.len(),
        limits.render.max_vertices,
        "object vertices",
    )?;
    *indices = add_limit(
        *indices,
        geometry.indices.len(),
        limits.render.max_indices,
        "object indices",
    )?;
    if geometry.indices.len() % 3 != 0 {
        return Err(ObjectError::Invalid("triangle index count"));
    }
    if geometry
        .indices
        .iter()
        .any(|&i| i as usize >= geometry.vertices.len())
    {
        return Err(ObjectError::Invalid("triangle index range"));
    }
    for v in &geometry.vertices {
        if !v.position.is_finite() || !v.uv.is_finite() || !v.normal.is_finite() {
            return Err(ObjectError::Invalid("nonfinite vertex"));
        }
        if body_bounds
            .map(|b| !b.contains(v.position))
            .unwrap_or(false)
        {
            return Err(ObjectError::Invalid("body outside source bounds"));
        }
    }
    Ok(())
}

fn prepare_geometry(source: NormalizedGeometry, version: u32) -> Result<Mesh, ObjectError> {
    let mut mesh = Mesh {
        vertices: source
            .vertices
            .into_iter()
            .map(|v| Vertex {
                position: v.position,
                uv: v.uv,
                normal: if version == 1 { Vec3::ZERO } else { v.normal },
                color: [1.; 4],
            })
            .collect(),
        indices: source.indices,
    };
    if version == 1 {
        // DGRP3DVert.GenerateNormals(false): sum unnormalized triangle crosses.
        // Keep f32 arithmetic; core's robust f64 normalization is a different policy.
        for tri in mesh.indices.chunks_exact(3) {
            let a = mesh.vertices[tri[0] as usize].position;
            let b = mesh.vertices[tri[1] as usize].position;
            let c = mesh.vertices[tri[2] as usize].position;
            let cross = (b - a).cross(c - b);
            for &id in tri {
                let v = &mut mesh.vertices[id as usize];
                v.normal = v.normal + cross;
            }
        }
        for v in &mut mesh.vertices {
            let n = v.normal;
            let length = ((n.x * n.x + n.y * n.y + n.z * n.z) as f64).sqrt() as f32;
            v.normal = n * (1. / length);
            // Degenerate or overflowing legacy normals were NaN/Inf in the source.
            // Reject that data atomically instead of admitting a poisoned mesh.
            if !v.normal.is_finite() || length == 0. || !length.is_finite() {
                return Err(ObjectError::Invalid("degenerate/overflowing v1 normals"));
            }
        }
    }
    Ok(mesh)
}

fn hash_f32(hash: &mut Sha256, value: f32) {
    hash.update(value.to_bits().to_le_bytes());
}
fn hash_vec3(hash: &mut Sha256, value: Vec3) {
    for value in [value.x, value.y, value.z] {
        hash_f32(hash, value);
    }
}
fn hash_mesh(hash: &mut Sha256, mesh: &Mesh) {
    hash.update((mesh.vertices.len() as u64).to_le_bytes());
    hash.update((mesh.indices.len() as u64).to_le_bytes());
    for vertex in &mesh.vertices {
        hash_vec3(hash, vertex.position);
        hash_vec3(hash, vertex.normal);
        hash_f32(hash, vertex.uv.x);
        hash_f32(hash, vertex.uv.y);
    }
    for &index in &mesh.indices {
        hash.update(index.to_le_bytes());
    }
}
fn content_key(mesh: &PreparedFsom) -> AssetKey {
    let mut hash = Sha256::new();
    hash.update(b"wonderland-normalized-fsom-v1\0");
    hash.update(mesh.identity.effective_source.0);
    hash.update(mesh.identity.effective_content.0);
    match mesh.context {
        FsomContext::Dgrp {
            effective_iff,
            chunk_id,
        } => {
            hash.update([1]);
            hash.update(effective_iff.0);
            hash.update(chunk_id.to_le_bytes());
        }
        FsomContext::Standalone => hash.update([0]),
    }
    hash.update(mesh.format_version.to_le_bytes());
    hash.update(mesh.reconstruction_version.to_le_bytes());
    hash_vec3(&mut hash, mesh.bounds.min);
    hash_vec3(&mut hash, mesh.bounds.max);
    hash.update((mesh.textures.len() as u64).to_le_bytes());
    for texture in &mesh.textures {
        match texture.selector() {
            TextureSelector::Sprite { rotation, ordinal } => {
                hash.update([0]);
                hash.update(rotation.to_le_bytes());
                hash.update(ordinal.to_le_bytes());
            }
            TextureSelector::Custom { id } => {
                hash.update([1]);
                hash.update(id.to_le_bytes());
            }
        }
        hash.update(texture.effective_asset().0);
        hash_f32(&mut hash, texture.uv_scale().x);
        hash_f32(&mut hash, texture.uv_scale().y);
        hash.update(texture.image().width.to_le_bytes());
        hash.update(texture.image().height.to_le_bytes());
        for pixel in &texture.image().pixels {
            hash.update(pixel);
        }
    }
    hash.update((mesh.groups.len() as u64).to_le_bytes());
    for group in &mesh.groups {
        hash.update((group.len() as u64).to_le_bytes());
        for part in group {
            hash.update((part.texture as u64).to_le_bytes());
            hash_mesh(&mut hash, &part.mesh);
        }
    }
    if let Some((kind, geometry)) = &mesh.depth_mask {
        hash.update([match kind {
            MaskType::Normal => 1,
            MaskType::Portal => 2,
            MaskType::None => 0,
        }]);
        hash_mesh(&mut hash, geometry);
    } else {
        hash.update([0]);
    }
    AssetKey(hash.finalize().into())
}
