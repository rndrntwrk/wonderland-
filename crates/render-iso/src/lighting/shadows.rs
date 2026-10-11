//! Source ShadowGeometry.cs/GradVertex.cs/gradpoly2D.fx. Owned immutable
//! geometry replaces the source's shared mutable scratch arrays.
use super::LightingBudget;
use crate::{IsoError, Rect, Result};
use wonderland_render_core::{cache::DerivedKey, AssetKey, RgbaImage, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowLightKind {
    Point,
    Directional,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLight {
    pub kind: ShadowLightKind,
    pub position_sixteenths: Vec2,
    pub direction: Vec2,
    pub radius_sixteenths: f32,
    pub falloff_multiplier: f32,
}
impl ShadowLight {
    pub(super) fn validate(self) -> Result<()> {
        if !bounded(self.position_sixteenths)
            || !self.direction.is_finite()
            || !self.radius_sixteenths.is_finite()
            || !(1. ..=1_000_000.).contains(&self.radius_sixteenths)
            || !self.falloff_multiplier.is_finite()
            || !(0. ..=10_000.).contains(&self.falloff_multiplier)
            || (self.kind == ShadowLightKind::Directional
                && (dot(self.direction, self.direction) - 1.).abs() > 0.0001)
        {
            return Err(IsoError::Invalid("shadow light"));
        }
        Ok(())
    }
    pub(super) fn bounds(self) -> Rect {
        Rect {
            x: (self.position_sixteenths.x - self.radius_sixteenths).trunc(),
            y: (self.position_sixteenths.y - self.radius_sixteenths).trunc(),
            width: self.radius_sixteenths.trunc() * 2.,
            height: self.radius_sixteenths.trunc() * 2.,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowGradient {
    Wall,
    DirectionalWall,
    ObjectEllipse,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowVertex {
    pub position: Vec2,
    pub color1: [u8; 4],
    pub color2: [u8; 4],
    pub start: Vec2,
    pub end: Vec2,
    pub parameters: [f32; 4],
    pub ellipse: [f32; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowMesh {
    key: DerivedKey,
    vertices: Vec<ShadowVertex>,
    indices: Vec<u32>,
    gradient: ShadowGradient,
}
impl ShadowMesh {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn vertices(&self) -> &[ShadowVertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn gradient(&self) -> ShadowGradient {
        self.gradient
    }
    pub fn resident_bytes(&self) -> usize {
        self.vertices.capacity() * std::mem::size_of::<ShadowVertex>()
            + self.indices.capacity() * std::mem::size_of::<u32>()
    }
    /// Explicit little-endian source GradVertex layout, with no Rust padding or unsafe casts.
    pub fn vertex_bytes(&self) -> Vec<u8> {
        encode_vertices(&self.vertices)
    }
    pub fn sample_occlusion(&self, position: Vec2, max_triangle_tests: usize) -> Result<f32> {
        if !bounded(position) {
            return Err(IsoError::Invalid("shadow sample"));
        }
        if self.indices.len() / 3 > max_triangle_tests {
            return Err(IsoError::Limit("shadow sample work"));
        }
        let mut result = 0.;
        for tri in self.indices.chunks_exact(3) {
            let a = self.vertices[tri[0] as usize];
            if contains_triangle(
                a.position,
                self.vertices[tri[1] as usize].position,
                self.vertices[tri[2] as usize].position,
                position,
            ) {
                let value = gradient_value(a, position, self.gradient);
                result = if self.gradient == ShadowGradient::ObjectEllipse {
                    f32::max(result, value)
                } else {
                    (result + value).min(1.)
                };
            }
        }
        Ok(result)
    }
}

fn bounded(p: Vec2) -> bool {
    p.is_finite() && p.x.abs() <= 1_000_000. && p.y.abs() <= 1_000_000.
}
fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}
fn length(v: Vec2) -> f32 {
    f64::from(dot(v, v)).sqrt() as f32
}
fn normal(v: Vec2) -> Vec2 {
    let l = length(v);
    if l == 0. {
        Vec2::ZERO
    } else {
        v * (1. / l)
    }
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - b.x * a.y
}
fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = (f64::from(angle).sin() as f32, f64::from(angle).cos() as f32);
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

pub fn generate_wall_shadows(
    walls: &[[Vec2; 2]],
    light: ShadowLight,
    budget: LightingBudget,
) -> Result<ShadowMesh> {
    light.validate()?;
    if walls.len() > budget.max_walls {
        return Err(IsoError::Limit("shadow walls"));
    }
    let mut builder = Builder::new(
        if light.kind == ShadowLightKind::Point {
            ShadowGradient::Wall
        } else {
            ShadowGradient::DirectionalWall
        },
        budget,
    );
    for &[a, b] in walls {
        if !bounded(a) || !bounded(b) || a == b {
            return Err(IsoError::Invalid("shadow wall segment"));
        }
        let bounds = light.bounds();
        if light.kind == ShadowLightKind::Point
            && (a.x.min(b.x) >= bounds.x + bounds.width
                || a.x.max(b.x) <= bounds.x
                || a.y.min(b.y) >= bounds.y + bounds.height
                || a.y.max(b.y) <= bounds.y)
        {
            continue;
        }
        let points = if cross(a - light.position_sixteenths, b - light.position_sixteenths) > 0. {
            [a, b, b]
        } else {
            [b, a, a]
        };
        builder.volume(points, light, None)?;
    }
    builder.finish()
}

pub fn generate_object_shadows(
    objects: &[Rect],
    light: ShadowLight,
    budget: LightingBudget,
) -> Result<ShadowMesh> {
    light.validate()?;
    if objects.len() > budget.max_occluders {
        return Err(IsoError::Limit("shadow objects"));
    }
    let mut builder = Builder::new(ShadowGradient::ObjectEllipse, budget);
    for &rect in objects {
        if !super::valid_rect(rect)
            || !bounded(Vec2::new(rect.x, rect.y))
            || !bounded(Vec2::new(rect.x + rect.width, rect.y + rect.height))
            || [rect.x, rect.y, rect.width, rect.height]
                .iter()
                .any(|x| x.fract() != 0.)
        {
            return Err(IsoError::Invalid("source object shadow rectangle"));
        }
        if light.kind == ShadowLightKind::Point && !rect.intersects(light.bounds()) {
            continue;
        }
        let p = light.position_sixteenths;
        // The original topDown collection is never emitted by GenerateShadows.
        if p.x >= rect.x && p.x < rect.x + rect.width && p.y >= rect.y && p.y < rect.y + rect.height
        {
            continue;
        }
        let corners = [
            Vec2::new(rect.x, rect.y),
            Vec2::new(rect.x, rect.y + rect.height),
            Vec2::new(rect.x + rect.width, rect.y + rect.height),
            Vec2::new(rect.x + rect.width, rect.y),
        ];
        let base = f64::from(corners[0].y - p.y).atan2(f64::from(corners[0].x - p.x));
        let (mut best, mut opposite, mut high, mut low) = (0, 0, 0., 0.);
        for (i, corner) in corners.iter().enumerate().skip(1) {
            let angle = f64::from(corner.y - p.y).atan2(f64::from(corner.x - p.x));
            let mut difference = (base - angle).rem_euclid(std::f64::consts::TAU);
            if difference > std::f64::consts::PI {
                difference -= std::f64::consts::TAU;
            }
            let difference = difference as f32;
            if difference > high {
                high = difference;
                best = i;
            }
            if difference < low {
                low = difference;
                opposite = i;
            }
        }
        let mut points = [Vec2::ZERO; 3];
        for point in &mut points {
            *point = corners[best];
            if best != opposite {
                best = (best + 1) % 4;
            }
        }
        let center = Vec2::new(
            rect.x + (rect.width / 2.).trunc(),
            rect.y + (rect.height / 2.).trunc(),
        );
        let half_width =
            f64::from(rect.width * rect.width + rect.height * rect.height).sqrt() as f32 / 2.5;
        builder.volume(points, light, Some((center, half_width)))?;
    }
    builder.finish()
}

#[derive(Clone, Copy, Default)]
struct Ellipse {
    center: Vec2,
    dimensions: [f32; 4],
}
impl ShadowVertex {
    fn solid(position: Vec2, ellipse: Ellipse) -> Self {
        Self {
            position,
            color1: [255; 4],
            color2: [0; 4],
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            parameters: [0., 0., ellipse.center.x, ellipse.center.y],
            ellipse: ellipse.dimensions,
        }
    }
    fn cone(
        position: Vec2,
        start: Vec2,
        end: Vec2,
        half: bool,
        angle: f32,
        ellipse: Ellipse,
    ) -> Self {
        Self {
            position,
            color1: [0; 4],
            color2: if half { [127; 4] } else { [255; 4] },
            start,
            end,
            parameters: [2., angle, ellipse.center.x, ellipse.center.y],
            ellipse: ellipse.dimensions,
        }
    }
}
struct Builder {
    vertices: Vec<ShadowVertex>,
    indices: Vec<u32>,
    gradient: ShadowGradient,
    budget: LightingBudget,
}
impl Builder {
    fn new(gradient: ShadowGradient, budget: LightingBudget) -> Self {
        Self {
            vertices: vec![],
            indices: vec![],
            gradient,
            budget,
        }
    }
    fn append(&mut self, vertices: &[ShadowVertex], indices: &[u32]) -> Result<()> {
        let v = self
            .vertices
            .len()
            .checked_add(vertices.len())
            .ok_or(IsoError::Limit("shadow vertices"))?;
        let i = self
            .indices
            .len()
            .checked_add(indices.len())
            .ok_or(IsoError::Limit("shadow indices"))?;
        if v > self.budget.max_vertices || v > u32::MAX as usize || i > self.budget.max_indices {
            return Err(IsoError::Limit("shadow geometry"));
        }
        // Worst case vector growth and canonical upload staging.
        self.budget.bytes(
            v.checked_mul(192)
                .and_then(|n| n.checked_add(i.checked_mul(12)?))
                .and_then(|n| n.checked_add(128))
                .ok_or(IsoError::Limit("shadow bytes"))?,
        )?;
        for vertex in vertices {
            if !vertex.position.is_finite()
                || !vertex.start.is_finite()
                || !vertex.end.is_finite()
                || vertex
                    .parameters
                    .iter()
                    .chain(&vertex.ellipse)
                    .any(|x| !x.is_finite())
            {
                return Err(IsoError::Invalid("shadow geometry arithmetic"));
            }
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(vertices);
        self.indices.extend(indices.iter().map(|i| base + i));
        Ok(())
    }
    fn volume(
        &mut self,
        pts: [Vec2; 3],
        light: ShadowLight,
        object: Option<(Vec2, f32)>,
    ) -> Result<()> {
        let penumbra = (std::f64::consts::PI / 12.) as f32;
        let pen_cos = f64::from(penumbra).cos() as f32;
        let p = penumbra * 2.;
        let directional = light.kind == ShadowLightKind::Directional;
        let distance = if directional && object.is_none() {
            32. * light.falloff_multiplier
        } else {
            5000.
        };
        if distance == 0. {
            return Ok(());
        }
        let mid = (pts[0] + pts[2]) * 0.5;
        let [left, center, right] = if directional {
            [light.direction; 3]
        } else {
            [
                normal(pts[0] - light.position_sixteenths),
                normal(mid - light.position_sixteenths),
                normal(pts[2] - light.position_sixteenths),
            ]
        };
        let ellipse = if let Some((ctr, width)) = object {
            let height = if directional {
                16. * light.falloff_multiplier
            } else {
                length(ctr - light.position_sixteenths) * 16. / 32.
            };
            let large = normal(ctr - light.position_sixteenths) * (width + height);
            let small = normal(Vec2::new(large.y, -large.x)) * width;
            Ellipse {
                center: ctr,
                dimensions: [small.x, small.y, large.x, large.y],
            }
        } else if directional {
            let perp = normal(Vec2::new(pts[2].y - pts[0].y, pts[0].x - pts[2].x));
            let length = distance * dot(perp, light.direction);
            if length == 0. {
                return Ok(());
            }
            let projected = perp * length;
            Ellipse {
                center: pts[0],
                dimensions: [length * length, 0., projected.x, projected.y],
            }
        } else {
            Ellipse::default()
        };
        let lp1 = pts[0] + rotate(left * distance, -penumbra);
        let lp2 = pts[0] + rotate(left * distance, penumbra);
        let rp1 = pts[2] + rotate(right * distance, -penumbra);
        let rp2 = pts[2] + rotate(right * distance, penumbra);
        let d1 = dot(
            pts[0] - light.position_sixteenths,
            pts[0] - light.position_sixteenths,
        );
        let d2 = dot(
            pts[2] - light.position_sixteenths,
            pts[2] - light.position_sixteenths,
        );
        if d1 > d2 && dot(normal(pts[0] - pts[2]), right) > pen_cos {
            let end = pts[2] + right;
            return self.append(
                &[
                    ShadowVertex::cone(pts[2], pts[2], end, true, p / 2., ellipse),
                    ShadowVertex::cone(rp1, pts[2], end, true, p / 2., ellipse),
                    ShadowVertex::cone(rp2, pts[2], end, true, p / 2., ellipse),
                ],
                &[0, 1, 2],
            );
        } else if d1 <= d2 && dot(normal(pts[2] - pts[0]), left) > pen_cos {
            let end = pts[0] + left;
            return self.append(
                &[
                    ShadowVertex::cone(pts[0], pts[0], end, true, p, ellipse),
                    ShadowVertex::cone(lp1, pts[0], end, true, p, ellipse),
                    ShadowVertex::cone(lp2, pts[0], end, true, p, ellipse),
                ],
                &[0, 1, 2],
            );
        }
        let a = lp2 - pts[0];
        let b = rp1 - pts[2];
        let perpendicular = Vec2::new(b.y, -b.x);
        let denominator = dot(a, perpendicular);
        // Parallel penumbras diverge; avoid the source divide-by-zero/NaN.
        let t = if denominator.abs() < 1e-12 {
            -1.
        } else {
            dot(pts[2] - pts[0], perpendicular) / denominator
        };
        if t < 0. {
            self.append(
                &[
                    ShadowVertex::solid(pts[0], ellipse),
                    ShadowVertex::solid(pts[1], ellipse),
                    ShadowVertex::solid(pts[2], ellipse),
                    ShadowVertex::solid(lp2, ellipse),
                    ShadowVertex::solid(pts[1] + center * distance, ellipse),
                    ShadowVertex::solid(rp1, ellipse),
                    ShadowVertex::cone(pts[0], pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(lp1, pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(lp2, pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(pts[2], pts[2], rp1, false, p, ellipse),
                    ShadowVertex::cone(rp1, pts[2], rp1, false, p, ellipse),
                    ShadowVertex::cone(rp2, pts[2], rp1, false, p, ellipse),
                ],
                &[0, 2, 1, 0, 3, 2, 3, 4, 2, 4, 5, 2, 6, 7, 8, 9, 10, 11],
            )
        } else {
            let intersection = pts[0] + a * t;
            let distant = mid + normal(intersection - light.position_sixteenths) * distance;
            self.append(
                &[
                    ShadowVertex::solid(pts[0], ellipse),
                    ShadowVertex::solid(intersection, ellipse),
                    ShadowVertex::solid(pts[2], ellipse),
                    ShadowVertex::solid(pts[1], ellipse),
                    ShadowVertex::cone(pts[0], pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(lp1, pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(intersection, pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(distant, pts[0], lp2, false, p, ellipse),
                    ShadowVertex::cone(pts[2], pts[2], rp1, false, p, ellipse),
                    ShadowVertex::cone(rp2, pts[2], rp1, false, p, ellipse),
                    ShadowVertex::cone(intersection, pts[2], rp1, false, p, ellipse),
                    ShadowVertex::cone(distant, pts[2], rp1, false, p, ellipse),
                ],
                &[0, 1, 2, 0, 2, 3, 4, 5, 6, 5, 7, 6, 8, 9, 10, 9, 11, 10],
            )
        }
    }
    fn finish(self) -> Result<ShadowMesh> {
        let key_capacity = self.vertices.len() * 64 + self.indices.len() * 4 + 17;
        self.budget
            .bytes(self.vertices.capacity() * 64 + self.indices.capacity() * 4 + key_capacity)?;
        let mut parameters = Vec::with_capacity(key_capacity);
        encode_vertices_into(&self.vertices, &mut parameters);
        parameters.extend_from_slice(&(self.vertices.len() as u64).to_le_bytes());
        parameters.extend_from_slice(&(self.indices.len() as u64).to_le_bytes());
        for index in &self.indices {
            parameters.extend_from_slice(&index.to_le_bytes());
        }
        parameters.push(match self.gradient {
            ShadowGradient::Wall => 0,
            ShadowGradient::DirectionalWall => 1,
            ShadowGradient::ObjectEllipse => 2,
        });
        let key = DerivedKey::new(AssetKey([0; 32]), AssetKey([0; 32]), 1, &parameters);
        Ok(ShadowMesh {
            key,
            vertices: self.vertices,
            indices: self.indices,
            gradient: self.gradient,
        })
    }
}
fn encode_vertices(vertices: &[ShadowVertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * 64);
    encode_vertices_into(vertices, &mut bytes);
    bytes
}
fn encode_vertices_into(vertices: &[ShadowVertex], bytes: &mut Vec<u8>) {
    for v in vertices {
        for f in [v.position.x, v.position.y] {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        bytes.extend_from_slice(&v.color1);
        bytes.extend_from_slice(&v.color2);
        for f in [v.start.x, v.start.y, v.end.x, v.end.y]
            .iter()
            .chain(&v.parameters)
            .chain(&v.ellipse)
        {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }
}
fn contains_triangle(a: Vec2, mut b: Vec2, mut c: Vec2, p: Vec2) -> bool {
    let area = cross(b - a, c - a);
    if area == 0. {
        return false;
    }
    if area < 0. {
        std::mem::swap(&mut b, &mut c);
    }
    [(a, b), (b, c), (c, a)].iter().all(|&(a, b)| {
        let edge = cross(b - a, p - a);
        edge > 0. || (edge == 0. && (b.y < a.y || (b.y == a.y && b.x > a.x)))
    })
}
fn gradient_value(v: ShadowVertex, p: Vec2, kind: ShadowGradient) -> f32 {
    let mut result = f32::from(v.color1[0]) / 255.;
    if v.parameters[0] == 2. {
        let diff = p - v.start;
        let angle = if diff == Vec2::ZERO {
            0.
        } else {
            dot(normal(v.end - v.start), normal(diff))
                .clamp(-1., 1.)
                .acos()
                / v.parameters[1]
        };
        let second = f32::from(v.color2[0]) / 255.;
        result = second + (result - second) * angle;
    }
    let pos = p - Vec2::new(v.parameters[2], v.parameters[3]);
    match kind {
        ShadowGradient::Wall => {}
        ShadowGradient::DirectionalWall => {
            let d = dot(pos, Vec2::new(v.ellipse[2], v.ellipse[3]));
            result *= if d < 0. || v.ellipse[0] == 0. {
                0.
            } else {
                1. - (d / v.ellipse[0]).clamp(0., 1.)
            };
        }
        ShadowGradient::ObjectEllipse => {
            let small = Vec2::new(v.ellipse[0], v.ellipse[1]);
            let large = Vec2::new(v.ellipse[2], v.ellipse[3]);
            let sl = length(small);
            let ll = length(large);
            if sl == 0. || ll == 0. {
                return 0.;
            }
            let sd = dot(pos, small) / (sl * sl);
            let mut ld = dot(pos, large) / ll;
            ld /= if ld < 0. { sl } else { ll };
            result *= 1. - length(Vec2::new(sd, ld)).clamp(0., 1.);
        }
    }
    result.clamp(0., 1.)
}
pub fn rasterize_shadow(
    mesh: &ShadowMesh,
    size: [u32; 2],
    extent_sixteenths: Vec2,
    budget: LightingBudget,
) -> Result<RgbaImage> {
    rasterize_shadow_counted(mesh, size, extent_sixteenths, budget).map(|v| v.0)
}
pub(super) fn rasterize_shadow_counted(
    mesh: &ShadowMesh,
    size: [u32; 2],
    extent_sixteenths: Vec2,
    budget: LightingBudget,
) -> Result<(RgbaImage, u64)> {
    let count = budget.texture(size[0], size[1])?;
    if !bounded(extent_sixteenths) || extent_sixteenths.x <= 0. || extent_sixteenths.y <= 0. {
        return Err(IsoError::Invalid("shadow target extent"));
    }
    budget.bytes(
        count
            .checked_mul(4)
            .and_then(|n| n.checked_add(mesh.resident_bytes()))
            .and_then(|n| n.checked_add(mesh.indices.len() / 3 * 16))
            .ok_or(IsoError::Limit("shadow target bytes"))?,
    )?;
    let scale = Vec2::new(
        size[0] as f32 / extent_sixteenths.x,
        size[1] as f32 / extent_sixteenths.y,
    );
    let mut ranges = Vec::with_capacity(mesh.indices.len() / 3);
    let mut work = count as u64;
    if work > budget.max_raster_samples {
        return Err(IsoError::Limit("shadow target clear work"));
    }
    for tri in mesh.indices.chunks_exact(3) {
        let vs = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize].position);
        let range = [
            (vs[0].x.min(vs[1].x).min(vs[2].x) * scale.x)
                .floor()
                .clamp(0., size[0] as f32) as u32,
            (vs[0].y.min(vs[1].y).min(vs[2].y) * scale.y)
                .floor()
                .clamp(0., size[1] as f32) as u32,
            (vs[0].x.max(vs[1].x).max(vs[2].x) * scale.x)
                .ceil()
                .clamp(0., size[0] as f32) as u32,
            (vs[0].y.max(vs[1].y).max(vs[2].y) * scale.y)
                .ceil()
                .clamp(0., size[1] as f32) as u32,
        ];
        work = work
            .checked_add(u64::from(range[2] - range[0]) * u64::from(range[3] - range[1]))
            .ok_or(IsoError::Limit("shadow raster work"))?;
        if work > budget.max_raster_samples {
            return Err(IsoError::Limit("shadow raster work"));
        }
        ranges.push(range);
    }
    let mut pixels = vec![[0, 0, 0, 255]; count];
    for (tri, range) in mesh.indices.chunks_exact(3).zip(ranges) {
        let a = mesh.vertices[tri[0] as usize];
        let b = mesh.vertices[tri[1] as usize];
        let c = mesh.vertices[tri[2] as usize];
        for y in range[1]..range[3] {
            for x in range[0]..range[2] {
                let p = Vec2::new((x as f32 + 0.5) / scale.x, (y as f32 + 0.5) / scale.y);
                if contains_triangle(a.position, b.position, c.position, p) {
                    let channel = if mesh.gradient == ShadowGradient::ObjectEllipse {
                        1
                    } else {
                        0
                    };
                    let out = &mut pixels[y as usize * size[0] as usize + x as usize][channel];
                    let sample = (gradient_value(a, p, mesh.gradient) * 255.).round() as u8;
                    *out = if channel == 1 {
                        (*out).max(sample)
                    } else {
                        out.saturating_add(sample)
                    };
                }
            }
        }
    }
    Ok((
        RgbaImage {
            width: size[0],
            height: size[1],
            pixels,
        },
        work,
    ))
}
