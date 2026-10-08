//! Validated borrowed light maps. There is no public unchecked image sampler.
//! The borrow prevents mutation of dimensions or pixels after validation.
use super::*;

#[derive(Clone, Copy)]
pub struct ReferenceLightmap<'a> {
    image: &'a RgbaImage,
    pub(crate) model_to_uv: Mat4,
}
impl<'a> ReferenceLightmap<'a> {
    pub fn new(
        image: &'a RgbaImage,
        model_to_uv: Mat4,
        limits: &RenderLimits,
    ) -> Result<Self, ReferenceError> {
        image.validate(limits)?;
        if !model_to_uv.is_finite() {
            return Err(ReferenceError::Invalid("lightmap transform"));
        }
        Ok(Self { image, model_to_uv })
    }
    /// Normalized point coordinates, linear filtering, clamp-to-edge. Alpha is
    /// deliberately not a visibility or coverage input.
    pub fn sample(&self, uv: Vec2) -> Result<[f32; 3], ReferenceError> {
        if !uv.is_finite() {
            return Err(ReferenceError::Invalid("lightmap sample"));
        }
        Ok(self.sample_valid(uv.x as f64, uv.y as f64))
    }
    pub(super) fn sample_valid(&self, u: f64, v: f64) -> [f32; 3] {
        // Clamp before multiplying: even a finite f32::MAX query is safe.
        let x = u.clamp(0., 1.) * f64::from(self.image.width) - 0.5;
        let y = v.clamp(0., 1.) * f64::from(self.image.height) - 0.5;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let fetch = |px: f64, py: f64| {
            let ix = px.clamp(0., f64::from(self.image.width - 1)) as usize;
            let iy = py.clamp(0., f64::from(self.image.height - 1)) as usize;
            self.image.pixels[iy * self.image.width as usize + ix]
        };
        let [a, b, c, d] = [
            fetch(x.floor(), y.floor()),
            fetch(x.floor() + 1., y.floor()),
            fetch(x.floor(), y.floor() + 1.),
            fetch(x.floor() + 1., y.floor() + 1.),
        ];
        std::array::from_fn(|i| {
            ((f64::from(a[i]) * (1. - fx) * (1. - fy)
                + f64::from(b[i]) * fx * (1. - fy)
                + f64::from(c[i]) * (1. - fx) * fy
                + f64::from(d[i]) * fx * fy)
                / 255.) as f32
        })
    }
}
