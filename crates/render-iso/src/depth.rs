use crate::Rotation;
use crate::{sprite_depth_fraction, IsoError, Result};
use wonderland_render_core::{AssetKey, Mat4, Vec3};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepthInput {
    None,
    Constant(u8),
    Bytes {
        key: AssetKey,
        width: u32,
        height: u32,
        values: Vec<u8>,
    },
}
impl DepthInput {
    pub fn sample(&self, x: u32, y: u32) -> Option<u8> {
        match self {
            Self::None => None,
            Self::Constant(q) => Some(*q),
            Self::Bytes {
                width,
                height,
                values,
                ..
            } => {
                if x >= *width || y >= *height {
                    return None;
                }
                let i = (y as usize)
                    .checked_mul(*width as usize)?
                    .checked_add(x as usize)?;
                values.get(i).copied()
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedDepth {
    pub depth: f32,
    pub w: f32,
    pub clip: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthAnchors {
    pub back: ProjectedDepth,
    pub front: ProjectedDepth,
}
impl DepthAnchors {
    pub fn new(anchor: Vec3, offset: Vec3, rotation: Rotation, wvp: Mat4) -> Result<Self> {
        if !anchor.is_finite()
            || !offset.is_finite()
            || wvp.cols.iter().flatten().any(|x| !x.is_finite())
        {
            return Err(IsoError::Invalid("depth anchor inputs"));
        }
        let (back, dir) = match rotation {
            Rotation::TopLeft => (Vec3::new(0.15, 0., 0.15), Vec3::new(3., 0., 3.)),
            Rotation::TopRight => (Vec3::new(0.15, 0., 2.85), Vec3::new(3., 0., -3.)),
            Rotation::BottomRight => (Vec3::new(2.85, 0., 2.85), Vec3::new(-3., 0., -3.)),
            Rotation::BottomLeft => (Vec3::new(2.85, 0., 0.15), Vec3::new(-3., 0., 3.)),
        };
        let back = anchor + offset + back;
        Ok(Self {
            back: project_depth(back, wvp)?,
            front: project_depth(back + dir, wvp)?,
        })
    }
    pub fn sample_byte(self, q: u8) -> f32 {
        self.back.depth + sprite_depth_fraction(q) * (self.front.depth - self.back.depth)
    }
    pub fn sample_w(self, q: u8) -> f32 {
        self.back.w + sprite_depth_fraction(q) * (self.front.w - self.back.w)
    }
    /// Literal source inverse-WVP input. `screen_clip_xy` is v.screenPos, not
    /// framebuffer pixels. Output is passed to lighting without another /w.
    pub fn reconstruct_lighting_position(
        self,
        screen_clip_xy: wonderland_render_core::Vec2,
        q: u8,
        inverse_wvp: Mat4,
    ) -> Result<[f32; 4]> {
        if !screen_clip_xy.is_finite() || inverse_wvp.cols.iter().flatten().any(|x| !x.is_finite())
        {
            return Err(IsoError::Invalid("lighting reconstruction inputs"));
        }
        let w = self.sample_w(q);
        let depth = self.sample_byte(q);
        let result = inverse_wvp.transform_vec4([screen_clip_xy.x, screen_clip_xy.y, depth * w, w]);
        if result.iter().any(|x| !x.is_finite()) {
            return Err(IsoError::Invalid("lighting reconstruction overflow"));
        }
        Ok(result)
    }
}
fn project_depth(point: Vec3, matrix: Mat4) -> Result<ProjectedDepth> {
    let clip = matrix.transform_vec4([point.x, point.y, point.z, 1.]);
    if clip.iter().any(|x| !x.is_finite()) || clip[3] == 0. {
        return Err(IsoError::Invalid("depth clip coordinate"));
    }
    let mut depth = clip[2] / clip[3] - 1e-11 * (clip[0] + clip[1]);
    if depth.is_nan() {
        depth = 0.;
    }
    if !depth.is_finite() {
        return Err(IsoError::Invalid("depth overflow"));
    }
    Ok(ProjectedDepth {
        depth,
        w: clip[3],
        clip,
    })
}
pub fn restore_depth(sample: f32, translation: Vec3, rot_projection: Mat4) -> Result<f32> {
    if !sample.is_finite() {
        return Err(IsoError::Invalid("cached depth"));
    }
    let a = project_depth(translation, rot_projection)?;
    let b = project_depth(Vec3::ZERO, rot_projection)?;
    // Source restoration includes the null projection's x/y bias as well.
    let result = sample + a.depth - b.clip[2] / b.w - 1e-11 * (b.clip[0] + b.clip[1]);
    if !result.is_finite() {
        return Err(IsoError::Invalid("cache depth overflow"));
    }
    Ok(result)
}
