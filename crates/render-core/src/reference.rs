//! CPU reference rendering only; this module makes no GPU equivalence or timing claim.
//! Pixels use top-left origin, pixel-center coverage, straight byte RGBA source-over,
//! strict less-than depth by default and a top-left triangle fill rule. Source
//! object passes can opt into less-or-equal depth. No gamma transfer is applied.
use crate::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterVertex {
    pub position: Vec3,
    pub reciprocal_w: f32,
    pub color: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FragmentOptions {
    pub depth_test: bool,
    pub write_depth: bool,
    pub write_id: bool,
    pub alpha_cutoff: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthComparison {
    Less,
    LessEqual,
}
impl Default for FragmentOptions {
    fn default() -> Self {
        Self {
            depth_test: true,
            write_depth: true,
            write_id: true,
            alpha_cutoff: 0,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ReferenceError {
    Validation(ValidationError),
    Invalid(&'static str),
    Allocation,
}
impl From<ValidationError> for ReferenceError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}
impl std::fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ReferenceError {}
pub struct ReferenceSurface {
    image: RgbaImage,
    depths: Vec<f32>,
    ids: Vec<Option<EntityRef>>,
    depth_comparison: DepthComparison,
}
impl ReferenceSurface {
    pub fn set_depth_comparison(&mut self, comparison: DepthComparison) {
        self.depth_comparison = comparison;
    }
    pub fn new(width: u32, height: u32, limits: &RenderLimits) -> Result<Self, ReferenceError> {
        let count = RgbaImage::checked_pixel_count(width, height, limits)?;
        Ok(Self {
            image: RgbaImage {
                width,
                height,
                pixels: allocated(count, [0; 4])?,
            },
            depths: allocated(count, f32::INFINITY)?,
            ids: allocated(count, None)?,
            depth_comparison: DepthComparison::Less,
        })
    }
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }
    pub fn depths(&self) -> &[f32] {
        &self.depths
    }
    pub fn ids(&self) -> &[Option<EntityRef>] {
        &self.ids
    }
    pub fn clear(&mut self, color: [u8; 4]) {
        self.image.pixels.fill(color);
        self.depths.fill(f32::INFINITY);
        self.ids.fill(None);
    }
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.image.width && y < self.image.height)
            .then(|| y as usize * self.image.width as usize + x as usize)
    }
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        self.index(x, y).map(|i| self.image.pixels[i])
    }
    pub fn depth_at(&self, x: u32, y: u32) -> Option<f32> {
        self.index(x, y).map(|i| self.depths[i])
    }
    pub fn id_at(&self, x: u32, y: u32) -> Option<EntityRef> {
        self.index(x, y).and_then(|i| self.ids[i])
    }
    pub fn write_fragment(
        &mut self,
        x: u32,
        y: u32,
        depth: f32,
        rgba: [u8; 4],
        id: Option<EntityRef>,
        options: FragmentOptions,
    ) -> Result<bool, ReferenceError> {
        validate_id(id)?;
        validate_depth(depth)?;
        let Some(i) = self.index(x, y) else {
            return Ok(false);
        };
        let depth_fails = match self.depth_comparison {
            DepthComparison::Less => depth >= self.depths[i],
            DepthComparison::LessEqual => depth > self.depths[i],
        };
        if rgba[3] <= options.alpha_cutoff || (options.depth_test && depth_fails) {
            return Ok(false);
        }
        self.image.pixels[i] = source_over(rgba, self.image.pixels[i]);
        if options.write_depth {
            self.depths[i] = depth;
        }
        if options.write_id {
            self.ids[i] = id;
        }
        Ok(true)
    }
    /// Screen-space triangle: position.z is post-projection depth in 0..1.
    /// Color interpolation is perspective-correct when reciprocal_w differs.
    pub fn draw_triangle(
        &mut self,
        vertices: [RasterVertex; 3],
        id: Option<EntityRef>,
        options: FragmentOptions,
    ) -> Result<usize, ReferenceError> {
        self.triangle(vertices, [Vec2::ZERO; 3], None, id, options)
    }
    fn triangle(
        &mut self,
        mut vertices: [RasterVertex; 3],
        mut uvs: [Vec2; 3],
        texture: Option<&RgbaImage>,
        id: Option<EntityRef>,
        options: FragmentOptions,
    ) -> Result<usize, ReferenceError> {
        validate_id(id)?;
        for v in &vertices {
            if !v.position.is_finite()
                || !v.reciprocal_w.is_finite()
                || v.reciprocal_w <= 0.
                || !v.color.iter().all(|x| x.is_finite())
            {
                return Err(ReferenceError::Invalid("triangle vertex"));
            }
            validate_depth(v.position.z)?;
        }
        let mut area = edge(
            vertices[0],
            vertices[1],
            vertices[2].position.x as f64,
            vertices[2].position.y as f64,
        );
        if area == 0. {
            return Ok(0);
        }
        if area < 0. {
            vertices.swap(1, 2);
            uvs.swap(1, 2);
            area = -area;
        }
        let min_x = vertices
            .iter()
            .map(|v| v.position.x as f64)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.)
            .min(self.image.width as f64) as u32;
        let max_x = vertices
            .iter()
            .map(|v| v.position.x as f64)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.)
            .min(self.image.width as f64) as u32;
        let min_y = vertices
            .iter()
            .map(|v| v.position.y as f64)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.)
            .min(self.image.height as f64) as u32;
        let max_y = vertices
            .iter()
            .map(|v| v.position.y as f64)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.)
            .min(self.image.height as f64) as u32;
        let mut count = 0;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let edges = [
                    edge(vertices[1], vertices[2], px, py),
                    edge(vertices[2], vertices[0], px, py),
                    edge(vertices[0], vertices[1], px, py),
                ];
                if !covered(edges[0], vertices[1], vertices[2])
                    || !covered(edges[1], vertices[2], vertices[0])
                    || !covered(edges[2], vertices[0], vertices[1])
                {
                    continue;
                }
                let weights = edges.map(|e| e / area);
                let mut depth = 0f64;
                let mut divisor = 0f64;
                let mut color = [0f64; 4];
                let mut uv = [0f64; 2];
                for i in 0..3 {
                    depth += weights[i] * vertices[i].position.z as f64;
                    let w = weights[i] * vertices[i].reciprocal_w as f64;
                    divisor += w;
                    for (c, out) in color.iter_mut().enumerate() {
                        *out += w * vertices[i].color[c] as f64;
                    }
                    uv[0] += w * uvs[i].x as f64;
                    uv[1] += w * uvs[i].y as f64;
                }
                let mut normalized = color.map(|c| (c / divisor).clamp(0., 1.));
                if let Some(image) = texture {
                    let texel = nearest(image, uv[0] / divisor, uv[1] / divisor);
                    for c in 0..4 {
                        normalized[c] *= texel[c] as f64 / 255.;
                    }
                }
                let rgba = normalized.map(|c| (c * 255.).round() as u8);
                if self.write_fragment(x, y, depth.clamp(0., 1.) as f32, rgba, id, options)? {
                    count += 1;
                }
            }
        }
        Ok(count)
    }
    /// Two-sided model geometry clipped to -w<=x,y<=w and 0<=z<=w.
    pub fn draw_mesh(
        &mut self,
        mesh: &Mesh,
        clip_from_model: Mat4,
        id: Option<EntityRef>,
        options: FragmentOptions,
        limits: &RenderLimits,
    ) -> Result<usize, ReferenceError> {
        self.mesh(mesh, clip_from_model, None, id, options, limits)
    }
    /// Nearest, clamped normalized UVs; vertex color multiplies straight RGBA texture.
    pub fn draw_textured_mesh(
        &mut self,
        mesh: &Mesh,
        clip_from_model: Mat4,
        texture: &RgbaImage,
        id: Option<EntityRef>,
        options: FragmentOptions,
        limits: &RenderLimits,
    ) -> Result<usize, ReferenceError> {
        texture.validate(limits)?;
        self.mesh(mesh, clip_from_model, Some(texture), id, options, limits)
    }
    fn mesh(
        &mut self,
        mesh: &Mesh,
        clip_from_model: Mat4,
        texture: Option<&RgbaImage>,
        id: Option<EntityRef>,
        options: FragmentOptions,
        limits: &RenderLimits,
    ) -> Result<usize, ReferenceError> {
        mesh.validate(limits)?;
        validate_id(id)?;
        if !clip_from_model.is_finite() {
            return Err(ReferenceError::Invalid("clip matrix"));
        }
        // Validate all transformed positions before the first write.
        let mut transformed = Vec::new();
        transformed
            .try_reserve_exact(mesh.vertices.len())
            .map_err(|_| ReferenceError::Allocation)?;
        for v in &mesh.vertices {
            let p = clip_from_model.transform_vec4([v.position.x, v.position.y, v.position.z, 1.]);
            if !p.iter().all(|n| n.is_finite()) {
                return Err(ReferenceError::Invalid("transformed vertex"));
            }
            transformed.push(ClipVertex {
                clip: p.map(|n| n as f64),
                color: v.color.map(|n| n as f64),
                uv: [v.uv.x as f64, v.uv.y as f64],
            });
        }
        let mut count = 0;
        for triangle in mesh.indices.chunks_exact(3) {
            let mut polygon = vec![
                transformed[triangle[0] as usize],
                transformed[triangle[1] as usize],
                transformed[triangle[2] as usize],
            ];
            for plane in 0..6 {
                polygon = clip_polygon(&polygon, plane);
                if polygon.is_empty() {
                    break;
                }
            }
            // Singular homogeneous points have no finite projected position.
            polygon.retain(|v| v.clip[3] > 0.);
            if polygon.len() < 3 {
                continue;
            }
            let projected: Vec<_> = polygon
                .iter()
                .map(|v| {
                    let w = v.clip[3];
                    (
                        RasterVertex {
                            position: Vec3::new(
                                ((v.clip[0] / w + 1.) * 0.5 * self.image.width as f64) as f32,
                                ((1. - v.clip[1] / w) * 0.5 * self.image.height as f64) as f32,
                                (v.clip[2] / w).clamp(0., 1.) as f32,
                            ),
                            reciprocal_w: (1. / w).min(f32::MAX as f64) as f32,
                            color: v.color.map(|c| c as f32),
                        },
                        Vec2::new(v.uv[0] as f32, v.uv[1] as f32),
                    )
                })
                .collect();
            for i in 1..projected.len() - 1 {
                count += self.triangle(
                    [projected[0].0, projected[i].0, projected[i + 1].0],
                    [projected[0].1, projected[i].1, projected[i + 1].1],
                    texture,
                    id,
                    options,
                )?;
            }
        }
        Ok(count)
    }
    pub fn blit(
        &mut self,
        image: &RgbaImage,
        origin: [i32; 2],
        depth: f32,
        id: Option<EntityRef>,
        options: FragmentOptions,
        limits: &RenderLimits,
    ) -> Result<usize, ReferenceError> {
        image.validate(limits)?;
        validate_id(id)?;
        validate_depth(depth)?;
        let mut count = 0;
        for y in 0..image.height {
            for x in 0..image.width {
                let px = origin[0] as i64 + x as i64;
                let py = origin[1] as i64 + y as i64;
                if px < 0
                    || py < 0
                    || px >= self.image.width as i64
                    || py >= self.image.height as i64
                {
                    continue;
                }
                if self.write_fragment(
                    px as u32,
                    py as u32,
                    depth,
                    image.pixels[y as usize * image.width as usize + x as usize],
                    id,
                    options,
                )? {
                    count += 1;
                }
            }
        }
        Ok(count)
    }
    /// SHA-256 over dimensions, color, exact f32 depth bits and game IDs.
    pub fn digest(&self) -> AssetKey {
        let mut hash = Sha256::new();
        hash.update(b"wonderland-reference-v1\0");
        hash.update(self.image.width.to_le_bytes());
        hash.update(self.image.height.to_le_bytes());
        for i in 0..self.image.pixels.len() {
            hash.update(self.image.pixels[i]);
            hash.update(self.depths[i].to_bits().to_le_bytes());
            match self.ids[i] {
                Some(id) => {
                    hash.update([1]);
                    hash.update(id.object_id.to_le_bytes());
                    hash.update(id.generation.to_le_bytes());
                }
                None => hash.update([0]),
            }
        }
        AssetKey(hash.finalize().into())
    }
}
fn allocated<T: Clone>(count: usize, value: T) -> Result<Vec<T>, ReferenceError> {
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| ReferenceError::Allocation)?;
    out.resize(count, value);
    Ok(out)
}
fn validate_depth(depth: f32) -> Result<(), ReferenceError> {
    if !depth.is_finite() || !(0. ..=1.).contains(&depth) {
        Err(ReferenceError::Invalid("depth"))
    } else {
        Ok(())
    }
}
fn validate_id(id: Option<EntityRef>) -> Result<(), ReferenceError> {
    if id.is_some_and(|id| id.generation == 0) {
        Err(ReferenceError::Invalid("entity generation"))
    } else {
        Ok(())
    }
}
fn source_over(source: [u8; 4], destination: [u8; 4]) -> [u8; 4] {
    let sa = source[3] as f64 / 255.;
    let da = destination[3] as f64 / 255.;
    let alpha = sa + da * (1. - sa);
    let mut out = [0; 4];
    if alpha > 0. {
        for c in 0..3 {
            out[c] = ((source[c] as f64 * sa + destination[c] as f64 * da * (1. - sa)) / alpha)
                .round() as u8;
        }
        out[3] = (alpha * 255.).round() as u8;
    }
    out
}
fn edge(a: RasterVertex, b: RasterVertex, x: f64, y: f64) -> f64 {
    // Evaluate a shared edge in one canonical orientation, then negate. Merely
    // exchanging endpoints in the cross-product expression can round differently
    // and create a crack or two fragments on the same pixel-center boundary.
    if a.position.x > b.position.x || (a.position.x == b.position.x && a.position.y > b.position.y)
    {
        return -canonical_edge(b, a, x, y);
    }
    canonical_edge(a, b, x, y)
}
fn canonical_edge(a: RasterVertex, b: RasterVertex, x: f64, y: f64) -> f64 {
    (b.position.x as f64 - a.position.x as f64) * (y - a.position.y as f64)
        - (b.position.y as f64 - a.position.y as f64) * (x - a.position.x as f64)
}
fn covered(value: f64, a: RasterVertex, b: RasterVertex) -> bool {
    let dy = b.position.y as f64 - a.position.y as f64;
    let dx = b.position.x as f64 - a.position.x as f64;
    value > 0. || (value == 0. && (dy < 0. || (dy == 0. && dx > 0.)))
}
fn nearest(image: &RgbaImage, u: f64, v: f64) -> [u8; 4] {
    let x = ((u.clamp(0., 1.) * image.width as f64).floor() as u32).min(image.width - 1);
    let y = ((v.clamp(0., 1.) * image.height as f64).floor() as u32).min(image.height - 1);
    image.pixels[y as usize * image.width as usize + x as usize]
}
#[derive(Clone, Copy)]
struct ClipVertex {
    clip: [f64; 4],
    color: [f64; 4],
    uv: [f64; 2],
}
fn plane_distance(v: ClipVertex, plane: usize) -> f64 {
    match plane {
        0 => v.clip[3] + v.clip[0],
        1 => v.clip[3] - v.clip[0],
        2 => v.clip[3] + v.clip[1],
        3 => v.clip[3] - v.clip[1],
        4 => v.clip[2],
        _ => v.clip[3] - v.clip[2],
    }
}
fn clip_polygon(input: &[ClipVertex], plane: usize) -> Vec<ClipVertex> {
    let mut out = Vec::new();
    let Some(&last) = input.last() else {
        return out;
    };
    let mut previous = last;
    let mut previous_distance = plane_distance(previous, plane);
    for &current in input {
        let current_distance = plane_distance(current, plane);
        if (previous_distance >= 0.) != (current_distance >= 0.) {
            let mut v = if previous_distance == 0. {
                previous
            } else if current_distance == 0. {
                current
            } else {
                let denominator = previous_distance - current_distance;
                let intersect =
                    |a: f64, b: f64| (b * previous_distance - a * current_distance) / denominator;
                let mut v = previous;
                for i in 0..4 {
                    v.clip[i] = intersect(previous.clip[i], current.clip[i]);
                    v.color[i] = intersect(previous.color[i], current.color[i]);
                }
                for i in 0..2 {
                    v.uv[i] = intersect(previous.uv[i], current.uv[i]);
                }
                v
            };
            // An intersection lies exactly on this plane. Preserve that fact
            // through later clipping, especially where several planes meet.
            match plane {
                0 => v.clip[0] = -v.clip[3],
                1 => v.clip[0] = v.clip[3],
                2 => v.clip[1] = -v.clip[3],
                3 => v.clip[1] = v.clip[3],
                4 => v.clip[2] = 0.,
                _ => v.clip[2] = v.clip[3],
            }
            out.push(v);
        }
        if current_distance >= 0. {
            out.push(current);
        }
        previous = current;
        previous_distance = current_distance;
    }
    out.dedup_by(|a, b| a.clip == b.clip);
    if out.len() > 1 && out.first().map(|v| v.clip) == out.last().map(|v| v.clip) {
        out.pop();
    }
    out
}
