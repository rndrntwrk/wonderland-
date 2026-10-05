use crate::*;
use std::sync::Arc;
use wonderland_render_core::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }
    pub fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MaskInput {
    pub key: AssetKey,
    pub rgba: RgbaImage,
    /// Upload extent. Texels outside `rgba` are transparent zero padding.
    pub physical_size: [u32; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteAsset {
    pub key: AssetKey,
    pub rgba: RgbaImage,
    pub physical_size: [u32; 2],
    pub depth: DepthInput,
    pub mask: Option<MaskInput>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DgrpLayer {
    pub sprite_id: u32,
    pub frame_index: u32,
    pub sprite_offset: Vec2,
    pub object_offset: Vec3,
    pub flags: u32,
    pub asset: Option<Arc<SpriteAsset>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DgrpImage {
    pub direction: u8,
    pub zoom: Zoom,
    pub layers: Vec<Option<DgrpLayer>>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DgrpInstance {
    pub reference: EntityRef,
    pub visual_revision: u64,
    pub direction: u8,
    pub tile_position: Vec3,
    pub room: u16,
    pub base_room: Option<u16>,
    pub level: i16,
    pub visible: bool,
    pub selectable: bool,
    pub cutaway_hidden: bool,
    pub dynamic_base: u32,
    pub dynamic_count: u32,
    pub dynamic_masks: [u64; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteLayer {
    Static,
    Dynamic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderMode {
    NoDepth,
    ZSprite,
    Floor,
    Wall,
    Restore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendPolicy {
    Premultiplied,
    LegacyDynamicNonPremultiplied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthKey {
    None,
    Constant(u8),
    Texture(AssetKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialKey {
    pub color: AssetKey,
    pub depth: DepthKey,
    pub mask: Option<AssetKey>,
    pub mode: RenderMode,
    pub gamma: GammaMode,
    pub blend: BlendPolicy,
    pub lighting: LightingResources,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparePolicy {
    pub visible_level: i16,
    pub draw_oob: bool,
    pub wvp: Option<Mat4>,
    /// Graphics-space offset baked into every emitted world anchor. Adapters
    /// use zero additional WorldOffset when drawing these prepared vertices.
    pub world_offset: Vec3,
    pub gamma: GammaMode,
    pub layer: SpriteLayer,
    pub blend: BlendPolicy,
    pub lighting: LightingResources,
    pub max_sprites: usize,
    pub limits: RenderLimits,
}
impl Default for PreparePolicy {
    fn default() -> Self {
        Self {
            visible_level: 6,
            draw_oob: false,
            wvp: None,
            world_offset: Vec3::ZERO,
            gamma: GammaMode::Advanced,
            layer: SpriteLayer::Dynamic,
            blend: BlendPolicy::Premultiplied,
            lighting: LightingResources::default(),
            max_sprites: 16383,
            limits: RenderLimits::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSprite {
    pub reference: EntityRef,
    pub visual_revision: u64,
    pub sprite_id: u32,
    pub frame_index: u32,
    /// Final framebuffer pixel edges, with top-left (0,0) and precise zoom
    /// already applied. No source backend half-pixel translation is pending.
    pub rect: Rect,
    pub local_rect: Rect,
    pub mesh: Mesh,
    /// Resolved graphics-space anchor, including PreparePolicy.world_offset.
    pub world_anchor: Vec3,
    pub anchors: Option<DepthAnchors>,
    pub asset: Arc<SpriteAsset>,
    pub material: MaterialKey,
    pub room: u16,
    pub room_uv: Vec2,
    pub floor_index: i16,
    pub draw_order: f32,
    pub submission_index: usize,
    pub selectable: bool,
    pub layer: SpriteLayer,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedObject {
    pub sprites: Vec<PreparedSprite>,
    pub bounds: Option<Rect>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FragmentSample {
    pub premultiplied_color: [f32; 4],
    pub depth: Option<f32>,
    pub reference: EntityRef,
}
impl PreparedSprite {
    /// CPU material/depth reference at logical destination texel coordinates.
    /// Point/clamp color, raw depth and mask share the same mirrored UV. This is
    /// a fragment oracle, not a compiled or qualified GPU renderer.
    pub fn fragment(
        &self,
        x: u32,
        y: u32,
        light: [f32; 3],
        pass: AlphaPass,
    ) -> Option<FragmentSample> {
        let image = &self.asset.rgba;
        if x >= image.width
            || y >= image.height
            || self.mesh.vertices.len() != 4
            || self.asset.physical_size.contains(&0)
        {
            return None;
        }
        let flip = self.mesh.vertices[0].uv.x > self.mesh.vertices[1].uv.x;
        let x = if flip { image.width - 1 - x } else { x };
        let index = (y as usize)
            .checked_mul(image.width as usize)?
            .checked_add(x as usize)?;
        let color = *image.pixels.get(index)?;
        let mask = self.asset.mask.as_ref().and_then(|m| {
            if m.rgba.width == 0 || m.rgba.height == 0 || m.physical_size.contains(&0) {
                return None;
            }
            let u = (x as f32 + 0.5) / self.asset.physical_size[0] as f32;
            let v = (y as f32 + 0.5) / self.asset.physical_size[1] as f32;
            let mx = ((u * m.physical_size[0] as f32).floor() as u32)
                .min(m.physical_size[0] - 1);
            let my = ((v * m.physical_size[1] as f32).floor() as u32)
                .min(m.physical_size[1] - 1);
            // Clamp at the uploaded texture boundary. Padding inside that
            // boundary stays transparent instead of repeating a logical edge.
            if mx >= m.rgba.width || my >= m.rgba.height {
                return Some(0);
            }
            let index = (my as usize)
                .checked_mul(m.rgba.width as usize)?
                .checked_add(mx as usize)?;
            m.rgba.pixels.get(index).map(|p| p[3])
        });
        let room = if self.material.mode == RenderMode::NoDepth {
            65535
        } else {
            self.room
        };
        let shaded = shade_fragment(color, mask, light, room, self.material.gamma, pass)?;
        let depth = match &self.asset.depth {
            DepthInput::None => None,
            other => Some(self.anchors?.sample_byte(other.sample(x, y)?)),
        };
        Some(FragmentSample {
            premultiplied_color: shaded,
            depth,
            reference: self.reference,
        })
    }
}
pub fn select_image(
    images: &[DgrpImage],
    direction: u8,
    zoom: Zoom,
    rotation: Rotation,
) -> Option<&DgrpImage> {
    let rotated = direction.rotate_left(u32::from(rotation as u8) * 2);
    images
        .iter()
        .find(|image| image.direction == rotated && image.zoom == zoom)
}

impl SpriteAsset {
    pub fn validate(&self, limits: &RenderLimits) -> Result<()> {
        self.rgba
            .validate(limits)
            .map_err(|_| IsoError::Invalid("color image"))?;
        validate_physical(&self.rgba, self.physical_size, limits)?;
        if let DepthInput::Bytes {
            width,
            height,
            values,
            ..
        } = &self.depth
        {
            let count = RgbaImage::checked_pixel_count(*width, *height, limits)
                .map_err(|_| IsoError::Limit("depth pixels"))?;
            if *width != self.rgba.width || *height != self.rgba.height || values.len() != count {
                return Err(IsoError::Invalid("paired depth dimensions"));
            }
        }
        if let Some(mask) = &self.mask {
            mask.rgba
                .validate(limits)
                .map_err(|_| IsoError::Invalid("mask image"))?;
            validate_physical(&mask.rgba, mask.physical_size, limits)?;
        }
        Ok(())
    }
}
fn validate_physical(image: &RgbaImage, size: [u32; 2], limits: &RenderLimits) -> Result<()> {
    if size[0] < image.width || size[1] < image.height {
        return Err(IsoError::Invalid(
            "physical texture smaller than logical image",
        ));
    }
    RgbaImage::checked_pixel_count(size[0], size[1], limits)
        .map_err(|_| IsoError::Limit("physical texture size"))?;
    Ok(())
}

/// Consumes normalized B adapter data. Sprite offsets are already cast to f32,
/// object offsets remain raw content units. Frame.position is deliberately absent.
pub fn prepare_sprites(
    projection: &Projection,
    instance: &DgrpInstance,
    images: &[DgrpImage],
    policy: &PreparePolicy,
) -> Result<PreparedObject> {
    let projection = Projection::new(
        projection.zoom,
        projection.rotation,
        projection.precise_zoom,
        projection.center_tile,
        projection.viewport,
    )?;
    if !instance.tile_position.is_finite()
        || !policy.world_offset.is_finite()
        || instance.reference.generation == 0
        || instance.reference.object_id == 0
        || instance.level < 1
        || policy.visible_level < 1
    {
        return Err(IsoError::Invalid("sprite instance"));
    }
    dynamic_sprite_visible(
        0,
        instance.dynamic_base,
        instance.dynamic_count,
        instance.dynamic_masks,
    )
    .map_err(IsoError::Invalid)?;
    let mut output = PreparedObject {
        sprites: vec![],
        bounds: None,
    };
    if !instance.visible
        || instance.cutaway_hidden
        || instance.level > policy.visible_level
        || (instance.room == 0 && !policy.draw_oob)
        || (!policy.draw_oob
            && instance.tile_position.x < -2043.
            && instance.tile_position.y < -2043.)
    {
        return Ok(output);
    }
    let image = match select_image(
        images,
        instance.direction,
        projection.zoom,
        projection.rotation,
    ) {
        Some(image) => image,
        None => return Ok(output),
    };
    if image.layers.len() > policy.max_sprites
        || image.layers.len() > policy.limits.max_vertices / 4
        || image.layers.len() > policy.limits.max_indices / 6
    {
        return Err(IsoError::Limit("DGRP layers"));
    }
    let wvp = match policy.wvp {
        Some(m) => m,
        None => projection.world_view_projection()?,
    };
    let metrics = projection.metrics();
    let position = projection.project_tile(instance.tile_position);
    let scroll = projection.sprite_screen_offset();
    let mut room = if instance.room >= 65533 {
        instance.room
    } else {
        instance.base_room.unwrap_or(instance.room)
    };
    if room == 0 && policy.draw_oob {
        room = 0;
    }
    for (submission_index, layer) in image.layers.iter().enumerate() {
        let layer = match layer {
            Some(layer) => layer,
            None => continue,
        };
        if layer.sprite_id > u16::MAX as u32 {
            return Err(IsoError::Invalid("sprite id exceeds source ushort lookup"));
        }
        if !layer.sprite_offset.is_finite() || !layer.object_offset.is_finite() {
            return Err(IsoError::Invalid("DGRP offset"));
        }
        if !dynamic_sprite_visible(
            layer.sprite_id,
            instance.dynamic_base,
            instance.dynamic_count,
            instance.dynamic_masks,
        )
        .map_err(IsoError::Invalid)?
        {
            continue;
        }
        let asset = match &layer.asset {
            Some(asset) => asset,
            None => continue,
        };
        asset.validate(&policy.limits)?;
        let offset = units::dgrp_offset_to_tiles(layer.object_offset);
        // Cardinal rotation is exact, avoiding source sin/cos epsilon drift.
        let local = match instance.direction {
            4 => Vec3::new(-offset.y, offset.x, offset.z),
            16 => Vec3::new(-offset.x, -offset.y, offset.z),
            64 => Vec3::new(offset.y, -offset.x, offset.z),
            _ => offset,
        };
        let px = projection.project_tile(local);
        let x = metrics.cadge_size.x * 0.5 + layer.sprite_offset.x + px.x;
        let y = metrics.baseline - asset.rgba.height as f32 + layer.sprite_offset.y + px.y;
        if ![x, y, position.x, position.y]
            .iter()
            .all(|x| x.is_finite() && *x >= i32::MIN as f32 && *x < i32::MAX as f32)
        {
            return Err(IsoError::Invalid("sprite rectangle overflow"));
        }
        let local_rect = Rect {
            x: x.trunc(),
            y: y.trunc(),
            width: asset.rgba.width as f32,
            height: asset.rgba.height as f32,
        };
        let discrete = Vec2::new(
            local_rect.x + position.x.trunc(),
            local_rect.y + position.y.trunc(),
        ) + scroll;
        let dest = (discrete - projection.viewport * 0.5) * projection.precise_zoom
            + projection.viewport * 0.5;
        let rect = Rect {
            x: dest.x,
            y: dest.y,
            width: local_rect.width * projection.precise_zoom,
            height: local_rect.height * projection.precise_zoom,
        };
        let world_anchor = units::tile_to_graphics(instance.tile_position)
            + units::tile_to_graphics(local)
            + policy.world_offset;
        let (depth, mode, anchors) = match &asset.depth {
            DepthInput::None => (DepthKey::None, RenderMode::NoDepth, None),
            DepthInput::Constant(q) => (
                DepthKey::Constant(*q),
                RenderMode::ZSprite,
                Some(DepthAnchors::new(
                    world_anchor,
                    Vec3::ZERO,
                    projection.rotation,
                    wvp,
                )?),
            ),
            DepthInput::Bytes { key, .. } => (
                DepthKey::Texture(*key),
                RenderMode::ZSprite,
                Some(DepthAnchors::new(
                    world_anchor,
                    Vec3::ZERO,
                    projection.rotation,
                    wvp,
                )?),
            ),
        };
        let sprite_room = if layer.flags & 4 != 0 && room != 65534 && room != 65533 {
            65535
        } else {
            room
        };
        let material = MaterialKey {
            color: asset.key,
            depth,
            mask: asset.mask.as_ref().map(|m| m.key),
            mode,
            gamma: policy.gamma,
            blend: policy.blend,
            lighting: policy.lighting,
        };
        let u = asset.rgba.width as f32 / asset.physical_size[0] as f32;
        let v = asset.rgba.height as f32 / asset.physical_size[1] as f32;
        let (left, right) = if layer.flags & 1 != 0 {
            (u, 0.)
        } else {
            (0., u)
        };
        let mesh = Mesh {
            vertices: vec![
                vertex(rect.x, rect.y, left, 0.),
                vertex(rect.x + rect.width, rect.y, right, 0.),
                vertex(rect.x + rect.width, rect.y + rect.height, right, v),
                vertex(rect.x, rect.y + rect.height, left, v),
            ],
            indices: vec![0, 1, 3, 1, 2, 3],
        };
        mesh.validate(&policy.limits)
            .map_err(|_| IsoError::Invalid("prepared quad"))?;
        let draw_order = projection.draw_order(instance.tile_position);
        if !draw_order.is_finite() || !world_anchor.is_finite() {
            return Err(IsoError::Invalid("sprite derived coordinates"));
        }
        output.bounds = Some(match output.bounds {
            Some(b) => b.union(rect),
            None => rect,
        });
        output.sprites.push(PreparedSprite {
            reference: instance.reference,
            visual_revision: instance.visual_revision,
            sprite_id: layer.sprite_id,
            frame_index: layer.frame_index,
            rect,
            local_rect,
            mesh,
            world_anchor,
            anchors,
            asset: asset.clone(),
            material,
            room: sprite_room,
            room_uv: Vec2::new(
                f32::from(sprite_room % 256) / 256.,
                f32::from(sprite_room / 256) / 256.,
            ),
            floor_index: instance.level - 1,
            draw_order,
            submission_index,
            selectable: instance.selectable,
            layer: policy.layer,
        });
    }
    Ok(output)
}
fn vertex(x: f32, y: f32, u: f32, v: f32) -> Vertex {
    Vertex {
        position: Vec3::new(x, y, 0.),
        normal: Vec3::Z,
        uv: Vec2::new(u, v),
        color: [1.; 4],
    }
}
