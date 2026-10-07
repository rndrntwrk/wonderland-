//! Bounded CPU thumbnails/facade atlases and disposable asynchronous jobs.
//!
//! Inputs are normalized meshes and explicit day/night material colors. This is
//! not the legacy room-lighting, shadow, sprite-depth, or FSOM decoder pipeline.
//! Every effective input byte, camera, frame identity and layout parameter is
//! hashed. Prepared requests and finished artifacts are immutable after validation.
use crate::cache::DerivedKey;
use crate::frame::{FrameError, FrameStore};
use crate::reference::{DepthComparison, FragmentOptions, ReferenceError, ReferenceSurface};
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

pub mod fsof;
pub mod source;
pub mod upload;

pub const DERIVATIVE_ALGORITHM_VERSION: u32 = 2;
pub const WALL_PIXELS_PER_TILE: u32 = 8;
pub const WALL_HEIGHT: u32 = 22;
pub const WALL_ATLAS_WIDTH: u32 = 512;
pub const WALL_GAP: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum DerivativeError {
    Invalid(&'static str),
    Limit(&'static str),
    Frame(FrameError),
    Reference(ReferenceError),
    Validation(ValidationError),
    Allocation,
    Exhausted,
    Stale,
    Poisoned,
}
impl std::fmt::Display for DerivativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DerivativeError {}
impl From<FrameError> for DerivativeError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}
impl From<ValidationError> for DerivativeError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}
impl From<ReferenceError> for DerivativeError {
    fn from(value: ReferenceError) -> Self {
        Self::Reference(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightingPass {
    /// Provenance of the caller's prepared light/material state, not a request to
    /// mutate simulation time or switch authoritative object lights.
    pub provenance: AssetKey,
    pub time_of_day: f32,
    /// A uniform color multiplier. Room lighting/shadows must already be baked
    /// in supplied colors/textures; this adapter never approximates those passes.
    pub color_multiplier: [f32; 3],
}
impl LightingPass {
    pub const DAY: Self = Self {
        provenance: AssetKey([0; 32]),
        time_of_day: 0.5,
        color_multiplier: [1.; 3],
    };
    pub const NIGHT: Self = Self {
        provenance: AssetKey([0; 32]),
        time_of_day: 0.,
        color_multiplier: [1.; 3],
    };
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialPass {
    pub tint: [f32; 4],
    /// Straight RGBA bytes, normalized nearest/clamped UV sampling.
    pub texture: Option<RgbaImage>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivativeMaterial {
    pub provenance: AssetKey,
    pub day: MaterialPass,
    pub night: MaterialPass,
    pub alpha_cutoff: u8,
    pub write_depth: bool,
}
impl DerivativeMaterial {
    pub fn solid(day: [f32; 4], night: [f32; 4]) -> Self {
        Self {
            provenance: AssetKey([0; 32]),
            day: MaterialPass {
                tint: day,
                texture: None,
            },
            night: MaterialPass {
                tint: night,
                texture: None,
            },
            alpha_cutoff: 0,
            write_depth: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawLayer {
    Wall,
    /// Zero-based terrain floor.
    Floor(u8),
    GroundMask,
    /// Zero-based roof level.
    Roof(u8),
    /// One-based object floor, matching EntityProjection.level.
    Object(i16),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivativeDraw {
    /// None denotes architecture. Owned geometry must match a visible frame
    /// entity and its asset; `model` is then relative to that entity's transform.
    pub owner: Option<EntityRef>,
    pub source_asset: AssetKey,
    pub model: Mat4,
    pub mesh: Mesh,
    pub material: u32,
    /// Preserve source draw order by supplying draws in the intended order.
    pub layer: DrawLayer,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThumbnailRequest {
    pub width: u32,
    pub height: u32,
    /// Explicit column-major clip matrix, depth 0..1. The legacy thumbnail uses
    /// WorldCamera2D and buildable-area cropping; callers supply that camera if
    /// they have it. A synthetic fixture camera is not legacy thumbnail parity.
    pub clip_from_world: Mat4,
    pub clear: [u8; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutsideSide {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FacadeWall {
    /// Source room wall/fence endpoints in 1/16-tile units.
    pub points: [[i32; 2]; 2],
    pub floor: u8,
    /// Source InterpAltitude at the segment midpoint, in tile units.
    pub terrain_height: f32,
    /// Explicit result of the source room-map outside-side test.
    pub outside: OutsideSide,
    pub room_provenance: AssetKey,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FacadeRequest {
    pub lot_width: u16,
    pub lot_height: u16,
    pub floor_tiles: u16,
    pub floor_resolution_per_tile: u16,
    /// 1..=5; a sixth cell remains available for the original object overlay.
    pub stories: u8,
    pub floors_used: u8,
    pub roof_on_floor: bool,
    /// Already classified exterior room walls/fences, in source insertion order.
    pub walls: Vec<FacadeWall>,
    pub thumbnail: Option<ThumbnailRequest>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DerivativeOutput {
    Thumbnail(ThumbnailRequest),
    Facade(FacadeRequest),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivativeInput {
    pub frame: RenderFrame,
    pub source_provenance: AssetKey,
    pub lighting: [LightingPass; 2],
    pub materials: Vec<DerivativeMaterial>,
    pub draws: Vec<DerivativeDraw>,
    pub output: DerivativeOutput,
}
#[derive(Clone, Copy, Debug)]
pub struct DerivativeRenderLimits {
    pub render: RenderLimits,
    pub max_draws: usize,
    pub max_materials: usize,
    pub max_views: usize,
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    /// Conservative vertex/clip/raster work model, including both passes and
    /// up to seven fan triangles after clipping one source triangle.
    pub max_work_units: u64,
}
impl Default for DerivativeRenderLimits {
    fn default() -> Self {
        Self {
            render: RenderLimits::default(),
            max_draws: 4096,
            max_materials: 4096,
            max_views: 512,
            max_input_bytes: 128 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
            max_work_units: 256_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageRole {
    ThumbnailDay,
    ThumbnailNight,
    FloorDay,
    FloorNight,
    WallDay,
    WallNight,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivativeImage {
    pub role: ImageRole,
    pub image: RgbaImage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionRole {
    Thumbnail,
    Floor(u8),
    ObjectOverlay(u8),
    Wall(u32),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasRegion {
    pub role: RegionRole,
    /// X, Y, width, height in each phase's corresponding image.
    pub rect: [u32; 4],
    /// Matrix for a local cell surface. Wall views include the source -1 pixel
    /// vertical displacement; floor cells have a one-pixel scissor inset.
    pub clip_from_world: Mat4,
}

#[derive(Debug)]
pub struct PreparedDerivative {
    input: DerivativeInput,
    limits: DerivativeRenderLimits,
    key: DerivedKey,
    source_digest: AssetKey,
    plans: Vec<ViewPlan>,
    image_specs: Vec<(ImageRole, u32, u32)>,
    reservation_bytes: u64,
    work_units: u64,
    input_bytes: u64,
    output_bytes: u64,
    facade_geometry: Option<fsof::FacadeGeometry>,
    day_only: bool,
}
#[derive(Debug)]
struct ViewPlan {
    region: AtlasRegion,
    image_base: usize,
    inset: u32,
    bleed: bool,
    clear: [u8; 4],
    commands: Vec<(usize, Mat4)>,
}
#[derive(Debug)]
pub struct DerivativeArtifact {
    key: DerivedKey,
    source_digest: AssetKey,
    frame_stamp: FrameStamp,
    images: Vec<DerivativeImage>,
    regions: Vec<AtlasRegion>,
    digest: AssetKey,
    resident_bytes: u64,
    facade_geometry: Option<fsof::FacadeGeometry>,
}
impl DerivativeArtifact {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn source_digest(&self) -> AssetKey {
        self.source_digest
    }
    pub fn frame_stamp(&self) -> FrameStamp {
        self.frame_stamp
    }
    pub fn images(&self) -> &[DerivativeImage] {
        &self.images
    }
    pub fn regions(&self) -> &[AtlasRegion] {
        &self.regions
    }
    pub fn digest(&self) -> AssetKey {
        self.digest
    }
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }
    pub fn facade_geometry(&self) -> Option<&fsof::FacadeGeometry> {
        self.facade_geometry.as_ref()
    }
    pub fn to_fsof(&self, night_light_color: [u8; 4]) -> Result<fsof::Fsof, DerivativeError> {
        let geometry = self
            .facade_geometry
            .as_ref()
            .ok_or(DerivativeError::Invalid(
                "source facade geometry unavailable",
            ))?;
        let get = |role| {
            self.images
                .iter()
                .find(|image| image.role == role)
                .ok_or(DerivativeError::Invalid("facade atlas unavailable"))
        };
        let floor = &get(ImageRole::FloorDay)?.image;
        let wall = &get(ImageRole::WallDay)?.image;
        let bytes = |role| -> Result<Vec<u8>, DerivativeError> {
            let image = &get(role)?.image;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(image.pixels.len() * 4)
                .map_err(|_| DerivativeError::Allocation)?;
            bytes.extend(image.pixels.iter().flatten().copied());
            Ok(bytes)
        };
        Ok(fsof::Fsof {
            compression: fsof::TextureCompression::Rgba8,
            floor_width: floor.width,
            floor_height: floor.height,
            wall_width: wall.width,
            wall_height: wall.height,
            floor_texture: bytes(ImageRole::FloorDay)?,
            wall_texture: bytes(ImageRole::WallDay)?,
            night: if self
                .images
                .iter()
                .any(|image| image.role == ImageRole::FloorNight)
            {
                Some(fsof::FsofNight {
                    floor_texture: bytes(ImageRole::FloorNight)?,
                    wall_texture: bytes(ImageRole::WallNight)?,
                    light_color: night_light_color,
                })
            } else {
                None
            },
            geometry: geometry.clone(),
        })
    }
}
impl PreparedDerivative {
    pub fn new(
        input: DerivativeInput,
        limits: DerivativeRenderLimits,
    ) -> Result<Self, DerivativeError> {
        if input.draws.len() > limits.max_draws || input.materials.len() > limits.max_materials {
            return Err(DerivativeError::Limit("draws/materials"));
        }
        if input.frame.entities.len() > limits.render.max_entities {
            return Err(DerivativeError::Limit("frame entities"));
        }
        let mut validator = FrameStore::new(limits.render);
        validator.reset(input.frame.stamp.lot_id, input.frame.stamp.epoch);
        validator.admit(input.frame.clone())?;
        for light in input.lighting {
            normalized(&light.color_multiplier, "lighting multiplier")?;
            if !light.time_of_day.is_finite() || !(0. ..1.).contains(&light.time_of_day) {
                return Err(DerivativeError::Invalid("time of day"));
            }
        }
        let mut input_bytes = std::mem::size_of::<Self>() as u64;
        add_bytes(
            &mut input_bytes,
            input.frame.entities.capacity(),
            std::mem::size_of::<EntityProjection>(),
        )?;
        add_bytes(
            &mut input_bytes,
            input.materials.capacity(),
            std::mem::size_of::<DerivativeMaterial>(),
        )?;
        add_bytes(
            &mut input_bytes,
            input.draws.capacity(),
            std::mem::size_of::<DerivativeDraw>(),
        )?;
        for material in &input.materials {
            for pass in [&material.day, &material.night] {
                normalized(&pass.tint, "material tint")?;
                if let Some(texture) = &pass.texture {
                    texture.validate(&limits.render)?;
                    add_bytes(&mut input_bytes, texture.pixels.capacity(), 4)?;
                }
            }
        }
        for draw in &input.draws {
            draw.mesh.validate(&limits.render)?;
            if draw.material as usize >= input.materials.len() || !draw.model.is_finite() {
                return Err(DerivativeError::Invalid("draw material/model"));
            }
            for vertex in &draw.mesh.vertices {
                normalized(&vertex.color, "vertex color")?;
            }
            if let Some(owner) = draw.owner {
                let entity = validator
                    .entity(owner)
                    .ok_or(DerivativeError::Invalid("draw owner"))?;
                if entity.asset != draw.source_asset {
                    return Err(DerivativeError::Invalid("draw asset"));
                }
                if let DrawLayer::Object(level) = draw.layer {
                    if level != entity.level {
                        return Err(DerivativeError::Invalid("object floor"));
                    }
                }
            }
            add_bytes(
                &mut input_bytes,
                draw.mesh.vertices.capacity(),
                std::mem::size_of::<Vertex>(),
            )?;
            add_bytes(&mut input_bytes, draw.mesh.indices.capacity(), 4)?;
        }
        if let DerivativeOutput::Facade(facade) = &input.output {
            add_bytes(
                &mut input_bytes,
                facade.walls.capacity(),
                std::mem::size_of::<FacadeWall>(),
            )?;
        }
        if input_bytes > limits.max_input_bytes {
            return Err(DerivativeError::Limit("input bytes"));
        }
        let (mut plans, image_specs) = layout(&input.output, &limits)?;
        let mut output_bytes = std::mem::size_of::<DerivativeArtifact>() as u64;
        add_bytes(
            &mut output_bytes,
            plans.len(),
            std::mem::size_of::<AtlasRegion>(),
        )?;
        add_bytes(
            &mut output_bytes,
            image_specs.len(),
            std::mem::size_of::<DerivativeImage>(),
        )?;
        for &(_, width, height) in &image_specs {
            let pixels = RgbaImage::checked_pixel_count(width, height, &limits.render)?;
            add_bytes(&mut output_bytes, pixels, 4)?;
        }
        if output_bytes > limits.max_output_bytes {
            return Err(DerivativeError::Limit("output bytes"));
        }
        let mut work_units = 0u64;
        let mut max_surface_pixels = 0u64;
        let mut max_mesh_bytes = 0u64;
        for plan in &mut plans {
            let pixels = plan.region.rect[2] as u64 * plan.region.rect[3] as u64;
            max_surface_pixels = max_surface_pixels.max(pixels);
            for (index, draw) in input.draws.iter().enumerate() {
                if !selected(draw.layer, plan.region.role, &input.output) {
                    continue;
                }
                let world = if let Some(owner) = draw.owner {
                    let entity = validator
                        .entity(owner)
                        .ok_or(DerivativeError::Invalid("draw owner"))?;
                    if !entity.visible {
                        continue;
                    }
                    entity.transform.matrix() * draw.model
                } else {
                    draw.model
                };
                // Charge vertex preparation before walking any vertices.
                work_units = work_units
                    .checked_add(
                        (draw.mesh.vertices.len() as u64)
                            .checked_mul(3)
                            .ok_or(DerivativeError::Limit("render work"))?,
                    )
                    .ok_or(DerivativeError::Limit("render work"))?;
                if work_units > limits.max_work_units {
                    return Err(DerivativeError::Limit("render work"));
                }
                let clip = plan.region.clip_from_world * world;
                if !clip.is_finite() {
                    return Err(DerivativeError::Invalid("combined matrix"));
                }
                // Source PreparedWorld uses individual tile meshes. Bounding
                // their actual projected raster region avoids charging an
                // entire 576-square thumbnail for every two-triangle tile.
                // Positive homogeneous W preserves these convex bounds through
                // clipping; eye-plane crossings conservatively use all pixels.
                let raster_pixels =
                    projected_pixels(&draw.mesh, clip, plan.region.rect[2], plan.region.rect[3])?;
                let triangle_cost = raster_pixels
                    .checked_mul(7)
                    .and_then(|n| n.checked_add(64))
                    .and_then(|n| n.checked_mul(2))
                    .ok_or(DerivativeError::Limit("render work"))?;
                let cost = (draw.mesh.indices.len() as u64 / 3)
                    .checked_mul(triangle_cost)
                    .ok_or(DerivativeError::Limit("render work"))?;
                work_units = work_units
                    .checked_add(cost)
                    .ok_or(DerivativeError::Limit("render work"))?;
                if work_units > limits.max_work_units {
                    return Err(DerivativeError::Limit("render work"));
                }
                plan.commands
                    .try_reserve(1)
                    .map_err(|_| DerivativeError::Allocation)?;
                plan.commands.push((index, clip));
                // Shaded mesh copy plus reference transformed ClipVertex scratch.
                let scratch = (draw.mesh.vertices.len() as u64)
                    .checked_mul(128)
                    .and_then(|v| v.checked_add(draw.mesh.indices.len() as u64 * 4))
                    .ok_or(DerivativeError::Limit("scratch bytes"))?;
                max_mesh_bytes = max_mesh_bytes.max(scratch);
            }
            add_bytes(
                &mut input_bytes,
                plan.commands.capacity(),
                std::mem::size_of::<(usize, Mat4)>(),
            )?;
            if input_bytes > limits.max_input_bytes {
                return Err(DerivativeError::Limit("input bytes"));
            }
        }
        add_bytes(
            &mut input_bytes,
            plans.capacity(),
            std::mem::size_of::<ViewPlan>(),
        )?;
        add_bytes(
            &mut input_bytes,
            image_specs.capacity(),
            std::mem::size_of::<(ImageRole, u32, u32)>(),
        )?;
        if input_bytes > limits.max_input_bytes {
            return Err(DerivativeError::Limit("input bytes"));
        }
        // RGBA + depth + Option<EntityRef> plus cloned output pixels during a
        // render. The conservative 32 bytes per surface pixel covers both.
        let reservation_bytes = input_bytes
            .checked_add(output_bytes)
            .and_then(|v| v.checked_add(max_surface_pixels.checked_mul(32)?))
            .and_then(|v| v.checked_add(max_mesh_bytes))
            .and_then(|v| v.checked_add(8192))
            .ok_or(DerivativeError::Limit("reservation bytes"))?;
        let source_digest = hash_input(&input);
        let key = DerivedKey::new(
            input.source_provenance,
            input.frame.stamp.content,
            DERIVATIVE_ALGORITHM_VERSION,
            &source_digest.0,
        );
        Ok(Self {
            input,
            limits,
            key,
            source_digest,
            plans,
            image_specs,
            reservation_bytes,
            work_units,
            input_bytes,
            output_bytes,
            facade_geometry: None,
            day_only: false,
        })
    }
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    /// Use when the source has no independently prepared night state. The
    /// unavailable phase is neither rendered nor exported under a night label.
    pub fn day_only(mut self) -> Self {
        if !self.day_only {
            let mut hash = Sha256::new();
            hash.update(b"derivative-day-only-v1\0");
            hash.update(self.source_digest.0);
            self.source_digest = AssetKey(hash.finalize().into());
            self.key = DerivedKey::new(
                self.input.source_provenance,
                self.input.frame.stamp.content,
                DERIVATIVE_ALGORITHM_VERSION,
                &self.source_digest.0,
            );
            self.day_only = true;
        }
        self
    }
    pub fn source_digest(&self) -> AssetKey {
        self.source_digest
    }
    pub fn frame(&self) -> &RenderFrame {
        &self.input.frame
    }
    pub fn input(&self) -> &DerivativeInput {
        &self.input
    }
    pub fn reservation_bytes(&self) -> u64 {
        self.reservation_bytes
    }
    pub fn work_units(&self) -> u64 {
        self.work_units
    }
    pub fn regions(&self) -> impl Iterator<Item = &AtlasRegion> {
        self.plans.iter().map(|p| &p.region)
    }
    pub fn render(&self) -> Result<DerivativeArtifact, DerivativeError> {
        self.render_while(|| true)
    }
    fn render_while(
        &self,
        mut current: impl FnMut() -> bool,
    ) -> Result<DerivativeArtifact, DerivativeError> {
        if !current() {
            return Err(DerivativeError::Stale);
        }
        let mut state = DerivativeRenderState::new(self)?;
        loop {
            if let Some(artifact) = state.step(self, 128, &mut current)? {
                return Ok(artifact);
            }
        }
    }
    fn finish_images(
        &self,
        mut images: Vec<DerivativeImage>,
    ) -> Result<DerivativeArtifact, DerivativeError> {
        if self.day_only {
            images.retain(|image| {
                matches!(
                    image.role,
                    ImageRole::ThumbnailDay | ImageRole::FloorDay | ImageRole::WallDay
                )
            });
        }
        let regions: Vec<_> = self.plans.iter().map(|p| p.region.clone()).collect();
        let mut resident_bytes = std::mem::size_of::<DerivativeArtifact>() as u64;
        add_bytes(
            &mut resident_bytes,
            images.capacity(),
            std::mem::size_of::<DerivativeImage>(),
        )?;
        add_bytes(
            &mut resident_bytes,
            regions.capacity(),
            std::mem::size_of::<AtlasRegion>(),
        )?;
        let mut hash = Sha256::new();
        hash.update(b"wonderland-derivative-output-v1\0");
        hash.update(self.key.0);
        for image in &images {
            add_bytes(&mut resident_bytes, image.image.pixels.capacity(), 4)?;
            hash.update([image.role as u8]);
            hash.update(image.image.width.to_le_bytes());
            hash.update(image.image.height.to_le_bytes());
            for pixel in &image.image.pixels {
                hash.update(pixel);
            }
        }
        let facade_geometry = self.facade_geometry.clone();
        if let Some(geometry) = &facade_geometry {
            resident_bytes = resident_bytes
                .checked_add(geometry.resident_bytes())
                .ok_or(DerivativeError::Limit("resident bytes"))?;
            hash.update(b"source-facade-geometry\0");
            for mesh in [&geometry.floor, &geometry.wall] {
                hash.update((mesh.vertices.len() as u64).to_le_bytes());
                for v in &mesh.vertices {
                    for f in [
                        v.position.x,
                        v.position.y,
                        v.position.z,
                        v.uv.x,
                        v.uv.y,
                        v.normal.x,
                        v.normal.y,
                        v.normal.z,
                    ] {
                        hash.update(f.to_bits().to_le_bytes());
                    }
                }
                hash.update((mesh.indices.len() as u64).to_le_bytes());
                for &index in &mesh.indices {
                    hash.update(index.to_le_bytes());
                }
            }
        }
        Ok(DerivativeArtifact {
            key: self.key,
            source_digest: self.source_digest,
            frame_stamp: self.input.frame.stamp,
            images,
            regions,
            digest: AssetKey(hash.finalize().into()),
            resident_bytes,
            facade_geometry,
        })
    }
}

struct DerivativeRenderState {
    images: Vec<DerivativeImage>,
    surface: Option<ReferenceSurface>,
    view: usize,
    phase: usize,
    command: usize,
}
impl DerivativeRenderState {
    fn new(request: &PreparedDerivative) -> Result<Self, DerivativeError> {
        let mut images = Vec::new();
        images
            .try_reserve_exact(request.image_specs.len())
            .map_err(|_| DerivativeError::Allocation)?;
        for &(role, width, height) in &request.image_specs {
            let count = RgbaImage::checked_pixel_count(width, height, &request.limits.render)?;
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(count)
                .map_err(|_| DerivativeError::Allocation)?;
            pixels.resize(count, [0; 4]);
            images.push(DerivativeImage {
                role,
                image: RgbaImage {
                    width,
                    height,
                    pixels,
                },
            });
        }
        Ok(Self {
            images,
            surface: None,
            view: 0,
            phase: 0,
            command: 0,
        })
    }
    fn clear(&mut self) {
        self.images.clear();
        self.surface = None;
    }
    fn step(
        &mut self,
        request: &PreparedDerivative,
        max_draws: usize,
        current: &mut impl FnMut() -> bool,
    ) -> Result<Option<DerivativeArtifact>, DerivativeError> {
        let mut remaining = max_draws;
        while self.view < request.plans.len() {
            if !current() {
                return Err(DerivativeError::Stale);
            }
            let plan = &request.plans[self.view];
            let [x, y, width, height] = plan.region.rect;
            if self.surface.is_none() {
                let mut surface = ReferenceSurface::new(width, height, &request.limits.render)?;
                surface.set_depth_comparison(DepthComparison::LessEqual);
                surface.clear(plan.clear);
                self.surface = Some(surface);
            }
            let surface = self
                .surface
                .as_mut()
                .ok_or(DerivativeError::Invalid("derivative surface"))?;
            while self.command < plan.commands.len() {
                if !current() {
                    return Err(DerivativeError::Stale);
                }
                if remaining == 0 {
                    return Ok(None);
                }
                let (draw_index, clip) = plan.commands[self.command];
                let draw = &request.input.draws[draw_index];
                let material = &request.input.materials[draw.material as usize];
                let pass = if self.phase == 0 {
                    &material.day
                } else {
                    &material.night
                };
                let light = request.input.lighting[self.phase].color_multiplier;
                let mut mesh = draw.mesh.clone();
                for vertex in &mut mesh.vertices {
                    for (i, &multiplier) in light.iter().enumerate() {
                        vertex.color[i] *= pass.tint[i] * multiplier;
                    }
                    vertex.color[3] *= pass.tint[3];
                }
                let options = FragmentOptions {
                    depth_test: true,
                    write_depth: material.write_depth,
                    write_id: false,
                    alpha_cutoff: material.alpha_cutoff,
                };
                if let Some(texture) = &pass.texture {
                    surface.draw_textured_mesh(
                        &mesh,
                        clip,
                        texture,
                        None,
                        options,
                        &request.limits.render,
                    )?;
                } else {
                    surface.draw_mesh(&mesh, clip, None, options, &request.limits.render)?;
                }
                self.command += 1;
                remaining -= 1;
            }
            let target = &mut self.images[plan.image_base + self.phase].image;
            for row in plan.inset..height - plan.inset {
                let source = row as usize * width as usize;
                let dest = (row + y) as usize * target.width as usize + x as usize;
                let start = plan.inset as usize;
                let end = (width - plan.inset) as usize;
                target.pixels[dest + start..dest + end]
                    .copy_from_slice(&surface.image().pixels[source + start..source + end]);
            }
            if plan.bleed {
                bleed(target, plan.region.rect);
            }
            self.surface = None;
            self.command = 0;
            self.phase += 1;
            if self.phase == if request.day_only { 1 } else { 2 } {
                self.phase = 0;
                self.view += 1;
            }
            if remaining == 0 {
                return Ok(None);
            }
        }
        if !current() {
            return Err(DerivativeError::Stale);
        }
        request
            .finish_images(std::mem::take(&mut self.images))
            .map(Some)
    }
}

fn projected_pixels(
    mesh: &Mesh,
    clip: Mat4,
    width: u32,
    height: u32,
) -> Result<u64, DerivativeError> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    let mut positive = true;
    for vertex in &mesh.vertices {
        let p = vertex.position;
        let v = clip.transform_vec4([p.x, p.y, p.z, 1.]);
        if v.iter().any(|f| !f.is_finite()) {
            return Err(DerivativeError::Invalid("transformed vertex"));
        }
        if v[3] <= 0. {
            positive = false;
            continue;
        }
        for i in 0..2 {
            let value = v[i] as f64 / v[3] as f64;
            low[i] = low[i].min(value);
            high[i] = high[i].max(value);
        }
    }
    if !positive {
        return Ok(width as u64 * height as u64);
    }
    if mesh.vertices.is_empty() {
        return Ok(0);
    }
    let mut sizes = [0; 2];
    for (i, dimension) in [width, height].into_iter().enumerate() {
        // Outward padding covers f32 screen conversion and floor/ceil bounds.
        let a = (((low[i].clamp(-1., 1.) + 1.) * 0.5 * dimension as f64).floor() - 2.).max(0.);
        let b = (((high[i].clamp(-1., 1.) + 1.) * 0.5 * dimension as f64).ceil() + 2.)
            .min(dimension as f64);
        sizes[i] = (b - a).max(0.) as u64;
    }
    Ok(sizes[0] * sizes[1])
}

fn normalized(values: &[f32], name: &'static str) -> Result<(), DerivativeError> {
    if values
        .iter()
        .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
    {
        Err(DerivativeError::Invalid(name))
    } else {
        Ok(())
    }
}
fn add_bytes(total: &mut u64, count: usize, size: usize) -> Result<(), DerivativeError> {
    *total = total
        .checked_add(
            (count as u64)
                .checked_mul(size as u64)
                .ok_or(DerivativeError::Limit("byte accounting"))?,
        )
        .ok_or(DerivativeError::Limit("byte accounting"))?;
    Ok(())
}
fn plan(
    role: RegionRole,
    rect: [u32; 4],
    camera: Mat4,
    image_base: usize,
    inset: u32,
    bleed: bool,
    clear: [u8; 4],
) -> ViewPlan {
    ViewPlan {
        region: AtlasRegion {
            role,
            rect,
            clip_from_world: camera,
        },
        image_base,
        inset,
        bleed,
        clear,
        commands: Vec::new(),
    }
}
fn add_thumbnail(
    request: ThumbnailRequest,
    plans: &mut Vec<ViewPlan>,
    images: &mut Vec<(ImageRole, u32, u32)>,
    limits: &DerivativeRenderLimits,
) -> Result<(), DerivativeError> {
    RgbaImage::checked_pixel_count(request.width, request.height, &limits.render)?;
    if !request.clip_from_world.is_finite() || request.clip_from_world.inverse().is_none() {
        return Err(DerivativeError::Invalid("thumbnail camera"));
    }
    let base = images.len();
    images.extend([
        (ImageRole::ThumbnailDay, request.width, request.height),
        (ImageRole::ThumbnailNight, request.width, request.height),
    ]);
    plans.push(plan(
        RegionRole::Thumbnail,
        [0, 0, request.width, request.height],
        request.clip_from_world,
        base,
        0,
        false,
        request.clear,
    ));
    Ok(())
}
/// Ordered view commands and output image allocations for one derivative.
type DerivativeLayout = (Vec<ViewPlan>, Vec<(ImageRole, u32, u32)>);

fn layout(
    output: &DerivativeOutput,
    limits: &DerivativeRenderLimits,
) -> Result<DerivativeLayout, DerivativeError> {
    let mut plans = Vec::new();
    let mut images = Vec::new();
    match output {
        DerivativeOutput::Thumbnail(request) => {
            add_thumbnail(*request, &mut plans, &mut images, limits)?
        }
        DerivativeOutput::Facade(request) => {
            if request.lot_width == 0
                || request.lot_height == 0
                || request.floor_tiles == 0
                || request.floor_resolution_per_tile == 0
                || !(1..=5).contains(&request.stories)
                || request.floors_used == 0
                || request.floors_used > request.stories
            {
                return Err(DerivativeError::Invalid("facade dimensions/stories"));
            }
            let view_count = request
                .walls
                .len()
                .checked_add(
                    request.floors_used as usize + 1 + usize::from(request.thumbnail.is_some()),
                )
                .ok_or(DerivativeError::Limit("views"))?;
            if view_count > limits.max_views {
                return Err(DerivativeError::Limit("views"));
            }
            let dim = request.floor_tiles as u32 * request.floor_resolution_per_tile as u32;
            if dim < 3 {
                return Err(DerivativeError::Invalid("floor cell inset"));
            }
            let width = dim
                .checked_mul(3)
                .ok_or(DerivativeError::Limit("floor atlas"))?;
            let height = dim
                .checked_mul(2)
                .ok_or(DerivativeError::Limit("floor atlas"))?;
            RgbaImage::checked_pixel_count(width, height, &limits.render)?;
            images.extend([
                (ImageRole::FloorDay, width, height),
                (ImageRole::FloorNight, width, height),
            ]);
            let center = Vec3::new(
                request.lot_width as f32 * 1.5,
                0.,
                request.lot_height as f32 * 1.5,
            );
            let view = Mat4::look_at_rh(
                center + Vec3::new(0., 200., 0.),
                center,
                Vec3::new(0., 0., 1.),
            )
            .ok_or(DerivativeError::Invalid("floor camera"))?;
            let half = request.floor_tiles as f32 * 1.5;
            let camera = Mat4::orthographic_rh(-half, half, -half, half, 0., 400.)
                .ok_or(DerivativeError::Invalid("floor camera"))?
                * view;
            for index in 0..=request.floors_used {
                let role = if index == request.stories {
                    RegionRole::ObjectOverlay(index)
                } else {
                    RegionRole::Floor(index)
                };
                plans.push(plan(
                    role,
                    [(index as u32 % 3) * dim, (index as u32 / 3) * dim, dim, dim],
                    camera,
                    0,
                    1,
                    false,
                    [0; 4],
                ));
            }
            let mut bins: Vec<(u32, u32)> = Vec::new(); // used width and number of walls
            for (index, wall) in request.walls.iter().enumerate() {
                if wall.floor >= request.stories
                    || !wall.terrain_height.is_finite()
                    || wall
                        .points
                        .iter()
                        .flatten()
                        .any(|&v| !(-1_048_576..=1_048_576).contains(&v))
                {
                    return Err(DerivativeError::Invalid("facade wall"));
                }
                let a = Vec2::new(wall.points[0][0] as f32, wall.points[0][1] as f32);
                let b = Vec2::new(wall.points[1][0] as f32, wall.points[1][1] as f32);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let source_length = (dx * dx + dy * dy).sqrt();
                let physical = source_length / 16.;
                if !physical.is_finite() || physical <= 0. {
                    return Err(DerivativeError::Invalid("wall length"));
                }
                let length = round_even((physical * WALL_PIXELS_PER_TILE as f32) as f64)
                    .min(WALL_ATLAS_WIDTH as f64) as u32;
                if length == 0 {
                    return Err(DerivativeError::Invalid("subpixel wall"));
                }
                let mut selected_bin = None;
                for (bin, &(used, count)) in bins.iter().enumerate() {
                    let effective = length + if count > 0 { WALL_GAP } else { 0 };
                    if effective <= WALL_ATLAS_WIDTH - used {
                        selected_bin = Some(bin);
                        break;
                    }
                }
                let bin = selected_bin.unwrap_or_else(|| {
                    bins.push((0, 0));
                    bins.len() - 1
                });
                let (used, count) = &mut bins[bin];
                let before = if *count > 0 { WALL_GAP } else { 0 };
                let x = *used + before;
                let y = bin as u32 * (WALL_HEIGHT + WALL_GAP * 2);
                *used = (*used + before + length + WALL_GAP).min(WALL_ATLAS_WIDTH);
                *count += 1;
                let sign = if wall.outside == OutsideSide::Left {
                    1.
                } else {
                    -1.
                };
                let normal = Vec2::new(-dy / source_length * sign, dx / source_length * sign);
                let cx = (a.x + b.x) / 32.;
                let cy = (a.y + b.y) / 32.;
                let h = (wall.floor as f32 + 0.5) * 2.95 * 3. + wall.terrain_height * 3. + 0.2;
                let target = Vec3::new(cx * 3., h, cy * 3.);
                let eye = Vec3::new((cx + normal.x) * 3., h, (cy + normal.y) * 3.);
                let view = Mat4::look_at_rh(eye, target, Vec3::new(0., 1., 0.))
                    .ok_or(DerivativeError::Invalid("wall camera"))?;
                let half = 1.5 * physical;
                let ortho = Mat4::orthographic_rh(-half, half, -2.90 * 1.5, 2.90 * 1.5, 0., 6.)
                    .ok_or(DerivativeError::Invalid("wall camera"))?;
                // LotFacadeGenerator's atlas translation has +2/atlas_height.
                // In the local WALL_HEIGHT viewport this is exactly -1 pixel.
                let camera = Mat4::from_translation(Vec3::new(0., 2. / WALL_HEIGHT as f32, 0.))
                    * Mat4::from_scale(Vec3::new(sign, 1., 1.))
                    * ortho
                    * view;
                if !camera.is_finite() {
                    return Err(DerivativeError::Invalid("wall camera"));
                }
                plans.push(plan(
                    RegionRole::Wall(index as u32),
                    [x, y, length, WALL_HEIGHT],
                    camera,
                    2,
                    0,
                    true,
                    [0; 4],
                ));
            }
            let raw_height = (bins.len() as u32)
                .checked_mul(WALL_HEIGHT + WALL_GAP * 2)
                .and_then(|v| v.checked_sub(WALL_GAP * 2))
                .unwrap_or(1)
                .max(1);
            let wall_height = raw_height
                .checked_add(3)
                .ok_or(DerivativeError::Limit("wall atlas"))?
                / 4
                * 4;
            RgbaImage::checked_pixel_count(WALL_ATLAS_WIDTH, wall_height, &limits.render)?;
            images.extend([
                (ImageRole::WallDay, WALL_ATLAS_WIDTH, wall_height),
                (ImageRole::WallNight, WALL_ATLAS_WIDTH, wall_height),
            ]);
            if let Some(thumbnail) = request.thumbnail {
                add_thumbnail(thumbnail, &mut plans, &mut images, limits)?;
            }
        }
    }
    if plans.len() > limits.max_views {
        return Err(DerivativeError::Limit("views"));
    }
    Ok((plans, images))
}
fn round_even(value: f64) -> f64 {
    let low = value.floor();
    let fraction = value - low;
    if fraction < 0.5 || (fraction == 0.5 && low % 2. == 0.) {
        low
    } else {
        low + 1.
    }
}
fn selected(layer: DrawLayer, role: RegionRole, output: &DerivativeOutput) -> bool {
    match role {
        RegionRole::Thumbnail => true,
        RegionRole::Floor(floor) => {
            let roof_on_floor = matches!(output, DerivativeOutput::Facade(f) if f.roof_on_floor);
            let last = matches!(output, DerivativeOutput::Facade(f) if floor == f.stories - 1);
            match layer {
                DrawLayer::Floor(level) => level == floor,
                DrawLayer::GroundMask => floor == 0,
                DrawLayer::Object(level) => level == floor as i16 + 1,
                DrawLayer::Roof(level) => {
                    roof_on_floor && ((floor > 0 && level == floor - 1) || (last && level == floor))
                }
                _ => false,
            }
        }
        RegionRole::ObjectOverlay(_) => matches!(layer, DrawLayer::Object(1)),
        RegionRole::Wall(index) => {
            let floor = match output {
                DerivativeOutput::Facade(f) => f.walls[index as usize].floor,
                _ => return false,
            };
            matches!(layer, DrawLayer::Wall)
                || matches!(layer, DrawLayer::Object(level) if level >= floor as i16 - 5)
        }
    }
}
fn bleed(image: &mut RgbaImage, [x, y, width, height]: [u32; 4]) {
    let stride = image.width as usize;
    let left = x.saturating_sub(1);
    let right = (x + width).min(image.width - 1);
    for row in y..y + height {
        let base = row as usize * stride;
        if x > 0 {
            image.pixels[base + left as usize] = image.pixels[base + x as usize];
        }
        if x + width < image.width {
            image.pixels[base + right as usize] = image.pixels[base + (x + width - 1) as usize];
        }
    }
    if y > 0 {
        for column in left..=right {
            image.pixels[(y - 1) as usize * stride + column as usize] =
                image.pixels[y as usize * stride + column as usize];
        }
    }
    if y + height < image.height {
        for column in left..=right {
            image.pixels[(y + height) as usize * stride + column as usize] =
                image.pixels[(y + height - 1) as usize * stride + column as usize];
        }
    }
}

struct InputHash(Sha256);
impl InputHash {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn f32(&mut self, value: f32) {
        self.bytes(&value.to_bits().to_le_bytes());
    }
    fn matrix(&mut self, value: Mat4) {
        for column in value.cols {
            for v in column {
                self.f32(v);
            }
        }
    }
    fn entity(&mut self, value: Option<EntityRef>) {
        if let Some(v) = value {
            self.u8(1);
            self.u64(v.object_id as u64);
            self.u64(v.generation as u64);
        } else {
            self.u8(0);
        }
    }
    fn transform(&mut self, value: Transform) {
        for v in [
            value.translation.x,
            value.translation.y,
            value.translation.z,
            value.rotation.x,
            value.rotation.y,
            value.rotation.z,
            value.rotation.w,
            value.scale.x,
            value.scale.y,
            value.scale.z,
        ] {
            self.f32(v);
        }
    }
    fn thumbnail(&mut self, value: ThumbnailRequest) {
        self.u64(value.width as u64);
        self.u64(value.height as u64);
        self.matrix(value.clip_from_world);
        self.bytes(&value.clear);
    }
    fn material_pass(&mut self, value: &MaterialPass) {
        for v in value.tint {
            self.f32(v);
        }
        if let Some(texture) = &value.texture {
            self.u8(1);
            self.u64(texture.width as u64);
            self.u64(texture.height as u64);
            self.u64(texture.pixels.len() as u64);
            for pixel in &texture.pixels {
                self.bytes(pixel);
            }
        } else {
            self.u8(0);
        }
    }
}
fn hash_input(input: &DerivativeInput) -> AssetKey {
    let mut hash = InputHash(Sha256::new());
    hash.bytes(b"wonderland-derivative-input-v1\0");
    hash.bytes(&input.source_provenance.0);
    let stamp = input.frame.stamp;
    for v in [
        stamp.lot_id,
        stamp.epoch,
        stamp.tick,
        stamp.architecture_revision,
    ] {
        hash.u64(v);
    }
    hash.bytes(&stamp.content.0);
    hash.entity(input.frame.selected);
    hash.u64(input.frame.entities.len() as u64);
    for entity in &input.frame.entities {
        hash.entity(Some(entity.reference));
        hash.u64(entity.visual_revision);
        hash.transform(entity.transform);
        if let Some(previous) = entity.previous_transform {
            hash.u8(1);
            hash.transform(previous);
        } else {
            hash.u8(0);
        }
        hash.bytes(&entity.asset.0);
        hash.bytes(&entity.level.to_le_bytes());
        hash.u8(entity.visible as u8);
        hash.u8(entity.selectable as u8);
    }
    for light in input.lighting {
        hash.bytes(&light.provenance.0);
        hash.f32(light.time_of_day);
        for v in light.color_multiplier {
            hash.f32(v);
        }
    }
    hash.u64(input.materials.len() as u64);
    for material in &input.materials {
        hash.bytes(&material.provenance.0);
        hash.material_pass(&material.day);
        hash.material_pass(&material.night);
        hash.u8(material.alpha_cutoff);
        hash.u8(material.write_depth as u8);
    }
    hash.u64(input.draws.len() as u64);
    for draw in &input.draws {
        hash.entity(draw.owner);
        hash.bytes(&draw.source_asset.0);
        hash.matrix(draw.model);
        hash.u64(draw.material as u64);
        match draw.layer {
            DrawLayer::Wall => hash.u8(0),
            DrawLayer::Floor(i) => {
                hash.u8(1);
                hash.u8(i);
            }
            DrawLayer::GroundMask => hash.u8(2),
            DrawLayer::Roof(i) => {
                hash.u8(3);
                hash.u8(i);
            }
            DrawLayer::Object(i) => {
                hash.u8(4);
                hash.bytes(&i.to_le_bytes());
            }
        }
        hash.u64(draw.mesh.vertices.len() as u64);
        for vertex in &draw.mesh.vertices {
            for v in [
                vertex.position.x,
                vertex.position.y,
                vertex.position.z,
                vertex.normal.x,
                vertex.normal.y,
                vertex.normal.z,
                vertex.uv.x,
                vertex.uv.y,
            ] {
                hash.f32(v);
            }
            for v in vertex.color {
                hash.f32(v);
            }
        }
        hash.u64(draw.mesh.indices.len() as u64);
        for &index in &draw.mesh.indices {
            hash.u64(index as u64);
        }
    }
    match &input.output {
        DerivativeOutput::Thumbnail(request) => {
            hash.u8(0);
            hash.thumbnail(*request);
        }
        DerivativeOutput::Facade(request) => {
            hash.u8(1);
            for v in [
                request.lot_width,
                request.lot_height,
                request.floor_tiles,
                request.floor_resolution_per_tile,
            ] {
                hash.bytes(&v.to_le_bytes());
            }
            hash.u8(request.stories);
            hash.u8(request.floors_used);
            hash.u8(request.roof_on_floor as u8);
            hash.u64(request.walls.len() as u64);
            for wall in &request.walls {
                for point in wall.points {
                    for v in point {
                        hash.bytes(&v.to_le_bytes());
                    }
                }
                hash.u8(wall.floor);
                hash.f32(wall.terrain_height);
                hash.u8(wall.outside as u8);
                hash.bytes(&wall.room_provenance.0);
            }
            if let Some(thumbnail) = request.thumbnail {
                hash.u8(1);
                hash.thumbnail(thumbnail);
            } else {
                hash.u8(0);
            }
        }
    }
    AssetKey(hash.0.finalize().into())
}

/// Counts both queued requests and unsettled worker handles. Cancelled workers
/// retain their reservation until completion or Drop acknowledges their lifetime.
#[derive(Clone, Copy, Debug)]
pub struct QueueLimits {
    pub max_queued: usize,
    pub max_in_flight: usize,
    pub max_pending_bytes: u64,
    pub max_resident_entries: usize,
    pub max_resident_bytes: u64,
}
impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            max_queued: 32,
            max_in_flight: 2,
            max_pending_bytes: 256 * 1024 * 1024,
            max_resident_entries: 100,
            max_resident_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueStats {
    pub queued: usize,
    pub in_flight: usize,
    pub pending_bytes: u64,
    pub resident_entries: usize,
    pub retired_entries: usize,
    pub resident_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct DerivativeTicket {
    identity: Arc<()>,
    generation: u64,
    serial: u64,
    key: DerivedKey,
}
impl PartialEq for DerivativeTicket {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.generation == other.generation
            && self.serial == other.serial
            && self.key == other.key
    }
}
impl Eq for DerivativeTicket {}
impl DerivativeTicket {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
}
#[derive(Clone)]
pub struct DerivativeQueue {
    state: Arc<Mutex<QueueState>>,
}
struct Queued {
    ticket: DerivativeTicket,
    request: PreparedDerivative,
}
struct Running {
    key: DerivedKey,
    bytes: u64,
}
struct Resident {
    artifact: Arc<DerivativeArtifact>,
    serial: u64,
    pins: u32,
    last_used: u128,
}
struct QueueState {
    identity: Arc<()>,
    limits: QueueLimits,
    frame: FrameStore,
    generation: u64,
    serial: u64,
    queued: VecDeque<Queued>,
    running: BTreeMap<u64, Running>,
    wanted: BTreeMap<DerivedKey, u64>,
    pending_bytes: u64,
    resident: BTreeMap<DerivedKey, Resident>,
    // Reset/replace detaches held entries from lookup, but cannot erase the
    // memory owned by a client lease. Retired entries keep that memory budgeted.
    retired: BTreeMap<u64, Resident>,
    resident_bytes: u64,
    clock: u128,
}
impl QueueState {
    fn current(&self, ticket: &DerivativeTicket) -> bool {
        Arc::ptr_eq(&self.identity, &ticket.identity)
            && ticket.generation == self.generation
            && self.wanted.get(&ticket.key) == Some(&ticket.serial)
    }
    fn finish(&mut self, ticket: &DerivativeTicket) {
        if let Some(running) = self.running.remove(&ticket.serial) {
            self.pending_bytes -= running.bytes;
            if self.wanted.get(&running.key) == Some(&ticket.serial) {
                self.wanted.remove(&running.key);
            }
        }
    }
    fn cancel_key(&mut self, key: &DerivedKey) -> bool {
        let Some(serial) = self.wanted.remove(key) else {
            return false;
        };
        if let Some(index) = self
            .queued
            .iter()
            .position(|entry| entry.ticket.serial == serial)
        {
            if let Some(entry) = self.queued.remove(index) {
                self.pending_bytes -= entry.request.reservation_bytes;
            }
        }
        true
    }
    fn invalidate(&mut self) {
        for queued in self.queued.drain(..) {
            self.pending_bytes -= queued.request.reservation_bytes;
        }
        self.wanted.clear();
        for (_, entry) in std::mem::take(&mut self.resident) {
            if entry.pins > 0 {
                self.retired.insert(entry.serial, entry);
            } else {
                self.resident_bytes -= entry.artifact.resident_bytes;
            }
        }
    }
    fn install(
        &mut self,
        serial: u64,
        artifact: DerivativeArtifact,
    ) -> Result<(), DerivativeError> {
        let key = artifact.key;
        let bytes = artifact.resident_bytes;
        if bytes > self.limits.max_resident_bytes {
            return Err(DerivativeError::Limit("resident bytes"));
        }
        let replaced = self
            .resident
            .get(&key)
            .filter(|e| e.pins == 0)
            .map(|e| e.artifact.resident_bytes);
        let mut projected_bytes = self
            .resident_bytes
            .checked_sub(replaced.unwrap_or(0))
            .and_then(|n| n.checked_add(bytes))
            .ok_or(DerivativeError::Limit("resident bytes"))?;
        let mut projected_count = self
            .resident
            .len()
            .checked_add(self.retired.len())
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_sub(usize::from(replaced.is_some())))
            .ok_or(DerivativeError::Limit("resident entries"))?;
        let mut candidates: Vec<_> = self
            .resident
            .iter()
            .filter(|(k, e)| **k != key && e.pins == 0)
            .map(|(&k, e)| (e.last_used, k, e.artifact.resident_bytes))
            .collect();
        candidates.sort_by_key(|&(age, key, _)| (age, key));
        let mut evictions = Vec::new();
        for (_, candidate, cost) in candidates {
            if projected_bytes <= self.limits.max_resident_bytes
                && projected_count <= self.limits.max_resident_entries
            {
                break;
            }
            projected_bytes -= cost;
            projected_count -= 1;
            evictions.push(candidate);
        }
        if projected_bytes > self.limits.max_resident_bytes
            || projected_count > self.limits.max_resident_entries
        {
            return Err(DerivativeError::Limit("resident capacity"));
        }
        // All checks precede eviction; rejected completion keeps the old cache.
        for candidate in evictions {
            self.resident.remove(&candidate);
            self.cancel_key(&candidate);
        }
        if let Some(old) = self.resident.remove(&key) {
            if old.pins > 0 {
                self.retired.insert(old.serial, old);
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.resident.insert(
            key,
            Resident {
                artifact: Arc::new(artifact),
                serial,
                pins: 0,
                last_used: self.clock,
            },
        );
        self.resident_bytes = projected_bytes;
        Ok(())
    }
}
impl DerivativeQueue {
    pub fn new(limits: QueueLimits, render_limits: RenderLimits) -> Result<Self, DerivativeError> {
        Ok(Self {
            state: Arc::new(Mutex::new(QueueState {
                identity: Arc::new(()),
                limits,
                frame: FrameStore::new(render_limits),
                generation: 0,
                serial: 0,
                queued: VecDeque::new(),
                running: BTreeMap::new(),
                wanted: BTreeMap::new(),
                pending_bytes: 0,
                resident: BTreeMap::new(),
                retired: BTreeMap::new(),
                resident_bytes: 0,
                clock: 0,
            })),
        })
    }
    fn lock(&self) -> Result<MutexGuard<'_, QueueState>, DerivativeError> {
        self.state.lock().map_err(|_| DerivativeError::Poisoned)
    }
    /// A reset is a new lifetime even when lot/epoch/content return to the same
    /// values. No running payload or held artifact is hidden from accounting.
    pub fn reset(&self, lot_id: u64, epoch: u64) -> Result<(), DerivativeError> {
        let mut state = self.lock()?;
        let next = state
            .generation
            .checked_add(1)
            .ok_or(DerivativeError::Exhausted)?;
        state.frame.reset(lot_id, epoch);
        state.generation = next;
        state.invalidate();
        Ok(())
    }
    pub fn admit_frame(&self, frame: RenderFrame) -> Result<(), DerivativeError> {
        let mut state = self.lock()?;
        let changed = state.frame.current() != Some(&frame);
        let next = if changed {
            state
                .generation
                .checked_add(1)
                .ok_or(DerivativeError::Exhausted)?
        } else {
            state.generation
        };
        state.frame.admit(frame)?;
        if changed {
            state.generation = next;
            state.invalidate();
        }
        Ok(())
    }
    /// Same-key resubmission supersedes that exact attempt. A failed preflight
    /// leaves the old attempt intact; a running predecessor retains its bytes.
    pub fn submit(&self, request: PreparedDerivative) -> Result<DerivativeTicket, DerivativeError> {
        let mut state = self.lock()?;
        if state.frame.current() != Some(request.frame()) {
            return Err(DerivativeError::Stale);
        }
        let key = request.key;
        let old_index = state
            .queued
            .iter()
            .position(|entry| entry.ticket.key == key);
        let old_bytes = old_index
            .map(|index| state.queued[index].request.reservation_bytes)
            .unwrap_or(0);
        let projected_count = state.queued.len() - usize::from(old_index.is_some());
        if projected_count >= state.limits.max_queued {
            return Err(DerivativeError::Limit("queued requests"));
        }
        let projected_bytes = state
            .pending_bytes
            .checked_sub(old_bytes)
            .and_then(|n| n.checked_add(request.reservation_bytes))
            .ok_or(DerivativeError::Limit("pending bytes"))?;
        if projected_bytes > state.limits.max_pending_bytes {
            return Err(DerivativeError::Limit("pending bytes"));
        }
        let serial = state
            .serial
            .checked_add(1)
            .ok_or(DerivativeError::Exhausted)?;
        state
            .queued
            .try_reserve(1)
            .map_err(|_| DerivativeError::Allocation)?;
        let ticket = DerivativeTicket {
            identity: state.identity.clone(),
            generation: state.generation,
            serial,
            key,
        };
        if let Some(index) = old_index {
            state.queued.remove(index);
        }
        state.wanted.insert(key, serial);
        state.pending_bytes = projected_bytes;
        state.serial = serial;
        state.queued.push_back(Queued {
            ticket: ticket.clone(),
            request,
        });
        Ok(ticket)
    }
    pub fn cancel(&self, ticket: &DerivativeTicket) -> Result<bool, DerivativeError> {
        let mut state = self.lock()?;
        if !state.current(ticket) {
            return Ok(false);
        }
        state.wanted.remove(&ticket.key);
        if let Some(index) = state.queued.iter().position(|e| e.ticket == *ticket) {
            if let Some(entry) = state.queued.remove(index) {
                state.pending_bytes -= entry.request.reservation_bytes;
            }
        }
        Ok(true)
    }
    pub fn start_next(&self) -> Result<Option<DerivativeJob>, DerivativeError> {
        let mut state = self.lock()?;
        if state.running.len() >= state.limits.max_in_flight {
            return Ok(None);
        }
        let Some(entry) = state.queued.pop_front() else {
            return Ok(None);
        };
        state.running.insert(
            entry.ticket.serial,
            Running {
                key: entry.ticket.key,
                bytes: entry.request.reservation_bytes,
            },
        );
        Ok(Some(DerivativeJob {
            state: Arc::downgrade(&self.state),
            ticket: entry.ticket,
            request: entry.request,
            finished: false,
        }))
    }
    pub fn stats(&self) -> Result<QueueStats, DerivativeError> {
        let state = self.lock()?;
        Ok(QueueStats {
            queued: state.queued.len(),
            in_flight: state.running.len(),
            pending_bytes: state.pending_bytes,
            resident_entries: state.resident.len(),
            retired_entries: state.retired.len(),
            resident_bytes: state.resident_bytes,
        })
    }
    /// A lease pins the precise installed generation and owns that lifetime.
    /// Reset can hide it from lookup, but its bytes remain reserved until Drop.
    pub fn acquire(&self, key: &DerivedKey) -> Result<Option<DerivativeLease>, DerivativeError> {
        let mut state = self.lock()?;
        state.clock = state.clock.saturating_add(1);
        let clock = state.clock;
        let Some(entry) = state.resident.get_mut(key) else {
            return Ok(None);
        };
        entry.pins = entry
            .pins
            .checked_add(1)
            .ok_or(DerivativeError::Limit("pins"))?;
        entry.last_used = clock;
        Ok(Some(DerivativeLease {
            state: Arc::downgrade(&self.state),
            key: *key,
            serial: entry.serial,
            artifact: entry.artifact.clone(),
        }))
    }
    pub fn evict(&self, key: &DerivedKey) -> Result<bool, DerivativeError> {
        let mut state = self.lock()?;
        if state.resident.get(key).is_some_and(|entry| entry.pins > 0) {
            return Ok(false);
        }
        let mut removed = state.cancel_key(key);
        if let Some(entry) = state.resident.remove(key) {
            state.resident_bytes -= entry.artifact.resident_bytes;
            removed = true;
        }
        Ok(removed)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Installed,
    Stale,
}
pub struct DerivativeJob {
    state: Weak<Mutex<QueueState>>,
    ticket: DerivativeTicket,
    request: PreparedDerivative,
    finished: bool,
}
impl DerivativeJob {
    pub fn ticket(&self) -> &DerivativeTicket {
        &self.ticket
    }
    pub fn request(&self) -> &PreparedDerivative {
        &self.request
    }
    pub fn into_task(self) -> Result<DerivativeTask, DerivativeError> {
        let state = DerivativeRenderState::new(&self.request)?;
        Ok(DerivativeTask {
            job: Some(self),
            state,
        })
    }
    pub fn is_cancelled(&self) -> bool {
        self.state
            .upgrade()
            .and_then(|state| state.lock().ok().map(|state| !state.current(&self.ticket)))
            .unwrap_or(true)
    }
    pub fn execute(self) -> Result<Completion, DerivativeError> {
        match self.request.render_while(|| !self.is_cancelled()) {
            Ok(artifact) => self.complete(artifact),
            Err(DerivativeError::Stale) => Ok(Completion::Stale),
            Err(error) => Err(error),
        }
    }
    /// Consumes both the attempt and its candidate. Stale/failed candidates drop
    /// immediately and can never clear a newer same-key request.
    pub fn complete(mut self, artifact: DerivativeArtifact) -> Result<Completion, DerivativeError> {
        let Some(state) = self.state.upgrade() else {
            return Ok(Completion::Stale);
        };
        let mut state = state.lock().map_err(|_| DerivativeError::Poisoned)?;
        let current = state.current(&self.ticket);
        state.finish(&self.ticket);
        self.finished = true;
        if !current {
            return Ok(Completion::Stale);
        }
        if artifact.key != self.request.key {
            return Err(DerivativeError::Invalid("completion key"));
        }
        state.install(self.ticket.serial, artifact)?;
        Ok(Completion::Installed)
    }
}
impl Drop for DerivativeJob {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(state) = self.state.upgrade() {
                // Drop must return reservations even during unwinding.
                state
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .finish(&self.ticket);
            }
        }
    }
}
/// A cooperatively scheduled worker. Each call processes at most `max_draws`
/// mesh draws, keeping staging memory under the job's existing reservation.
pub struct DerivativeTask {
    job: Option<DerivativeJob>,
    state: DerivativeRenderState,
}
impl DerivativeTask {
    pub fn step(&mut self, max_draws: usize) -> Result<Option<Completion>, DerivativeError> {
        if max_draws == 0 {
            return Err(DerivativeError::Invalid("zero derivative step"));
        }
        let job = self
            .job
            .as_ref()
            .ok_or(DerivativeError::Invalid("finished derivative task"))?;
        match self
            .state
            .step(&job.request, max_draws, &mut || !job.is_cancelled())
        {
            Ok(Some(artifact)) => self
                .job
                .take()
                .ok_or(DerivativeError::Invalid("derivative job"))?
                .complete(artifact)
                .map(Some),
            Ok(None) => Ok(None),
            Err(DerivativeError::Stale) => {
                self.state.clear();
                self.job.take();
                Ok(Some(Completion::Stale))
            }
            Err(error) => {
                self.state.clear();
                self.job.take();
                Err(error)
            }
        }
    }
}
impl Drop for DerivativeTask {
    fn drop(&mut self) {
        self.state.clear();
        self.job.take();
    }
}
pub struct DerivativeLease {
    state: Weak<Mutex<QueueState>>,
    key: DerivedKey,
    serial: u64,
    artifact: Arc<DerivativeArtifact>,
}
impl DerivativeLease {
    pub fn artifact(&self) -> &DerivativeArtifact {
        &self.artifact
    }
    /// A held lease remains memory-owned after invalidation, but is no longer
    /// eligible for upload or display. This distinction closes A/B/A races.
    pub fn is_current(&self) -> bool {
        self.state
            .upgrade()
            .and_then(|state| {
                state.lock().ok().map(|state| {
                    state
                        .resident
                        .get(&self.key)
                        .is_some_and(|entry| entry.serial == self.serial)
                })
            })
            .unwrap_or(false)
    }
}
impl Drop for DerivativeLease {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut state = state.lock().unwrap_or_else(|poison| poison.into_inner());
            if let Some(entry) = state
                .resident
                .get_mut(&self.key)
                .filter(|entry| entry.serial == self.serial)
            {
                entry.pins -= 1;
            } else if let Some(entry) = state.retired.get_mut(&self.serial) {
                entry.pins -= 1;
                if entry.pins == 0 {
                    if let Some(entry) = state.retired.remove(&self.serial) {
                        state.resident_bytes -= entry.artifact.resident_bytes;
                    }
                }
            }
        }
    }
}
