#![forbid(unsafe_code)]
//! Pure, source-grounded isometric presentation. No engine or simulation state.

pub mod material;
pub use material::*;
pub mod projection;
pub use projection::*;
pub mod depth;
pub use depth::*;
pub mod sprites;
pub use sprites::*;
pub mod batch;
pub use batch::*;
pub mod cutaway;
pub use cutaway::*;
pub mod view;
pub use view::*;
pub mod lighting;
pub use lighting::*;
pub mod shader;
pub use shader::*;
pub mod invalidation;
pub use invalidation::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IsoError {
    Invalid(&'static str),
    Limit(&'static str),
}
impl std::fmt::Display for IsoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IsoError {}
pub type Result<T> = std::result::Result<T, IsoError>;

/// Source unorm conversion extrapolates beyond the front anchor for q < 153.
pub fn sprite_depth_fraction(q: u8) -> f32 {
    (1.0 - f32::from(q) / 255.0) / 0.4
}

pub fn dynamic_sprite_visible(
    sprite: u32,
    base: u32,
    count: u32,
    masks: [u64; 2],
) -> std::result::Result<bool, &'static str> {
    if count > 128 {
        return Err("dynamic sprite range exceeds two masks");
    }
    let end = base
        .checked_add(count)
        .ok_or("dynamic sprite range overflows")?;
    if sprite < base || sprite >= end {
        return Ok(true);
    }
    let index = sprite - base;
    Ok(masks[(index / 64) as usize] & (1u64 << (index % 64)) != 0)
}
