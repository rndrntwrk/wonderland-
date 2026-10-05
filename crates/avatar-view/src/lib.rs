#![forbid(unsafe_code)]
pub mod fixtures;
pub mod mesh;
pub mod normalized;
pub mod rig;
pub use mesh::*;
pub use normalized::*;
pub use rig::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvatarError {
    Limit(&'static str),
    Invalid(&'static str),
    MissingBone(String),
    Stale,
    MissingResource(String),
}
pub type Result<T> = std::result::Result<T, AvatarError>;
#[derive(Clone, Copy, Debug)]
pub struct AvatarLimits {
    pub max_bones: usize,
    pub max_vertices: usize,
    pub max_blends: usize,
    pub max_indices: usize,
    pub max_motions: usize,
    pub max_samples: usize,
    pub max_parts: usize,
    pub max_metadata_bytes: usize,
}
impl Default for AvatarLimits {
    fn default() -> Self {
        Self {
            max_bones: 50,
            max_vertices: 100_000,
            max_blends: 100_000,
            max_indices: 300_000,
            max_motions: 1024,
            max_samples: 1_000_000,
            max_parts: 128,
            max_metadata_bytes: 16 * 1024 * 1024,
        }
    }
}
pub(crate) fn unit(q: wonderland_render_core::math::Quat) -> bool {
    let n = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    n.is_finite() && (n - 1.0).abs() <= 0.001
}
pub mod animation;
pub use animation::*;
pub mod contact;
pub mod look;
pub use contact::*;
pub use look::*;
pub mod appearance;
pub use appearance::*;
pub mod cook;
pub mod lod;
pub mod preview;
pub use cook::*;
pub use lod::*;
pub use preview::*;

impl std::fmt::Display for AvatarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AvatarError {}
