use crate::math::{Mat4, Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityRef {
    pub object_id: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssetKey(pub [u8; 32]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameStamp {
    pub lot_id: u64,
    pub epoch: u64,
    pub tick: u64,
    pub architecture_revision: u64,
    pub content: AssetKey,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewMode {
    Full2D,
    Hybrid2D,
    Full3D,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };
    pub fn is_valid(self) -> bool {
        self.translation.is_finite()
            && self.rotation.is_unit()
            && self.scale.is_finite()
            && self.matrix().is_finite()
    }
    pub fn matrix(self) -> Mat4 {
        Mat4::from_trs(self.translation, self.rotation, self.scale)
    }
    pub fn interpolate(self, next: Self, fraction: f32) -> Option<Self> {
        if !fraction.is_finite()
            || !(0. ..=1.).contains(&fraction)
            || !self.is_valid()
            || !next.is_valid()
        {
            return None;
        }
        let out = Self {
            translation: self.translation.lerp(next.translation, fraction),
            rotation: self.rotation.slerp(next.rotation, fraction),
            scale: self.scale.lerp(next.scale, fraction),
        };
        out.is_valid().then_some(out)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityProjection {
    pub reference: EntityRef,
    pub visual_revision: u64,
    pub transform: Transform,
    pub previous_transform: Option<Transform>,
    pub asset: AssetKey,
    pub level: i16,
    pub visible: bool,
    pub selectable: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderFrame {
    pub stamp: FrameStamp,
    pub entities: Vec<EntityProjection>,
    pub selected: Option<EntityRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub color: [f32; 4],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderLimits {
    pub max_entities: usize,
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_image_dimension: u32,
    pub max_texture_pixels: usize,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_entities: 65535,
            max_vertices: 2_000_000,
            max_indices: 6_000_000,
            max_image_dimension: 4096,
            max_texture_pixels: 16_777_216,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    Limit(&'static str),
    Invalid(&'static str),
}
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ValidationError {}
impl Mesh {
    pub fn validate(&self, limits: &RenderLimits) -> Result<(), ValidationError> {
        if self.vertices.len() > limits.max_vertices {
            return Err(ValidationError::Limit("vertices"));
        }
        if self.indices.len() > limits.max_indices {
            return Err(ValidationError::Limit("indices"));
        }
        if self.indices.len() % 3 != 0 {
            return Err(ValidationError::Invalid("triangle index count"));
        }
        for v in &self.vertices {
            if !v.position.is_finite()
                || !v.normal.is_finite()
                || !v.uv.is_finite()
                || !v.color.iter().all(|x| x.is_finite())
            {
                return Err(ValidationError::Invalid("nonfinite vertex"));
            }
        }
        if self
            .indices
            .iter()
            .any(|&i| i as usize >= self.vertices.len())
        {
            return Err(ValidationError::Invalid("index range"));
        }
        Ok(())
    }
}
impl RgbaImage {
    pub fn checked_pixel_count(
        width: u32,
        height: u32,
        limits: &RenderLimits,
    ) -> Result<usize, ValidationError> {
        if width == 0 || height == 0 {
            return Err(ValidationError::Invalid("empty image"));
        }
        if width > limits.max_image_dimension || height > limits.max_image_dimension {
            return Err(ValidationError::Limit("image dimension"));
        }
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ValidationError::Limit("image pixels"))?;
        if count > limits.max_texture_pixels {
            return Err(ValidationError::Limit("image pixels"));
        }
        Ok(count)
    }
    pub fn validate(&self, limits: &RenderLimits) -> Result<(), ValidationError> {
        let count = Self::checked_pixel_count(self.width, self.height, limits)?;
        if self.pixels.len() != count {
            return Err(ValidationError::Invalid("pixel length"));
        }
        Ok(())
    }
}
