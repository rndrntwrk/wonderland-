//! Owned presentation math. Column-major matrices multiply column vectors.
use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}
impl Vec2 {
    pub const ZERO: Self = Self::new(0., 0.);
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Vec3 {
    pub const ZERO: Self = Self::new(0., 0., 0.);
    pub const ONE: Self = Self::new(1., 1., 1.);
    pub const X: Self = Self::new(1., 0., 0.);
    pub const Y: Self = Self::new(0., 1., 0.);
    pub const Z: Self = Self::new(0., 0., 1.);
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        ((self.x as f64).powi(2) + (self.y as f64).powi(2) + (self.z as f64).powi(2)).sqrt() as f32
    }
    pub fn normalize_or_zero(self) -> Self {
        if !self.is_finite() {
            return Self::ZERO;
        }
        let length =
            ((self.x as f64).powi(2) + (self.y as f64).powi(2) + (self.z as f64).powi(2)).sqrt();
        if length == 0. {
            Self::ZERO
        } else {
            Self::new(
                (self.x as f64 / length) as f32,
                (self.y as f64 / length) as f32,
                (self.z as f64 / length) as f32,
            )
        }
    }
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self * (1. - t) + rhs * t
    }
}
macro_rules! vector_ops {
    ($name:ident, $($field:ident),+) => {
        impl Add for $name {type Output=Self; fn add(self,rhs:Self)->Self { Self { $($field:self.$field+rhs.$field),+ } }}
        impl Sub for $name {type Output=Self; fn sub(self,rhs:Self)->Self { Self { $($field:self.$field-rhs.$field),+ } }}
        impl Mul<f32> for $name {type Output=Self; fn mul(self,rhs:f32)->Self { Self { $($field:self.$field*rhs),+ } }}
        impl Div<f32> for $name {type Output=Self; fn div(self,rhs:f32)->Self { Self { $($field:self.$field/rhs),+ } }}
        impl Neg for $name {type Output=Self; fn neg(self)->Self { Self { $($field:-self.$field),+ } }}
    }
}
vector_ops!(Vec2, x, y);
vector_ops!(Vec3, x, y, z);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
impl Quat {
    pub const IDENTITY: Self = Self::new(0., 0., 0., 1.);
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    pub fn is_finite(self) -> bool {
        [self.x, self.y, self.z, self.w]
            .iter()
            .all(|n| n.is_finite())
    }
    pub fn is_unit(self) -> bool {
        self.is_finite() && (self.dot(self) - 1.).abs() <= 1e-4
    }
    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Option<Self> {
        let axis = axis.normalize_or_zero();
        if axis == Vec3::ZERO || !angle.is_finite() {
            return None;
        }
        let (s, c) = (angle * 0.5).sin_cos();
        Self::new(axis.x * s, axis.y * s, axis.z * s, c).normalize()
    }
    pub fn normalize(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
        let len = ((self.x as f64).powi(2)
            + (self.y as f64).powi(2)
            + (self.z as f64).powi(2)
            + (self.w as f64).powi(2))
        .sqrt();
        if len == 0. {
            return None;
        }
        Some(Self::new(
            (self.x as f64 / len) as f32,
            (self.y as f64 / len) as f32,
            (self.z as f64 / len) as f32,
            (self.w as f64 / len) as f32,
        ))
    }
    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }
    /// Shortest-path interpolation; inputs must be unit quaternions and t finite.
    pub fn slerp(self, mut rhs: Self, t: f32) -> Self {
        let mut dot = self.dot(rhs);
        if dot < 0. {
            rhs = Self::new(-rhs.x, -rhs.y, -rhs.z, -rhs.w);
            dot = -dot;
        }
        let (a, b) = if dot > 0.9995 {
            (1. - t, t)
        } else {
            let angle = dot.clamp(-1., 1.).acos();
            let denominator = angle.sin();
            (
                ((1. - t) * angle).sin() / denominator,
                (t * angle).sin() / denominator,
            )
        };
        Self::new(
            self.x * a + rhs.x * b,
            self.y * a + rhs.y * b,
            self.z * a + rhs.z * b,
            self.w * a + rhs.w * b,
        )
        .normalize()
        .unwrap_or(Self::IDENTITY)
    }
    pub fn rotate_vec3(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.;
        let result = v + t * self.w + q.cross(t);
        if result.is_finite() {
            return result;
        }
        // Preserve ordinary source arithmetic, widening only when intermediate
        // cross products overflow even though the rotated vector is representable.
        let q = [self.x as f64, self.y as f64, self.z as f64];
        let p = [v.x as f64, v.y as f64, v.z as f64];
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross(q, p).map(|n| n * 2.);
        let c = cross(q, t);
        Vec3::new(
            (p[0] + t[0] * self.w as f64 + c[0]) as f32,
            (p[1] + t[1] * self.w as f64 + c[1]) as f32,
            (p[2] + t[2] * self.w as f64 + c[2]) as f32,
        )
    }
}
impl Mul for Quat {
    type Output = Self;
    fn mul(self, r: Self) -> Self {
        Self::new(
            self.w * r.x + self.x * r.w + self.y * r.z - self.z * r.y,
            self.w * r.y - self.x * r.z + self.y * r.w + self.z * r.x,
            self.w * r.z + self.x * r.y - self.y * r.x + self.z * r.w,
            self.w * r.w - self.x * r.x - self.y * r.y - self.z * r.z,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}
impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ],
    };
    pub fn is_finite(self) -> bool {
        self.cols.iter().flatten().all(|n| n.is_finite())
    }
    pub fn from_translation(v: Vec3) -> Self {
        let mut out = Self::IDENTITY;
        out.cols[3] = [v.x, v.y, v.z, 1.];
        out
    }
    pub fn from_scale(v: Vec3) -> Self {
        Self {
            cols: [
                [v.x, 0., 0., 0.],
                [0., v.y, 0., 0.],
                [0., 0., v.z, 0.],
                [0., 0., 0., 1.],
            ],
        }
    }
    pub fn from_quat(q: Quat) -> Self {
        let x = q.rotate_vec3(Vec3::X);
        let y = q.rotate_vec3(Vec3::Y);
        let z = q.rotate_vec3(Vec3::Z);
        Self {
            cols: [
                [x.x, x.y, x.z, 0.],
                [y.x, y.y, y.z, 0.],
                [z.x, z.y, z.z, 0.],
                [0., 0., 0., 1.],
            ],
        }
    }
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self::from_translation(translation) * Self::from_quat(rotation) * Self::from_scale(scale)
    }
    pub fn transpose(self) -> Self {
        let mut out = Self::IDENTITY;
        for c in 0..4 {
            for r in 0..4 {
                out.cols[c][r] = self.cols[r][c];
            }
        }
        out
    }
    pub fn inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
        let mut a = [[0f64; 8]; 4];
        for (r, row) in a.iter_mut().enumerate() {
            for (c, value) in row[..4].iter_mut().enumerate() {
                *value = self.cols[c][r] as f64;
            }
            row[r + 4] = 1.;
        }
        for c in 0..4 {
            let mut pivot = c;
            for r in c + 1..4 {
                if a[r][c].abs() > a[pivot][c].abs() {
                    pivot = r;
                }
            }
            if a[pivot][c] == 0. {
                return None;
            }
            a.swap(c, pivot);
            let divisor = a[c][c];
            for value in &mut a[c] {
                *value /= divisor;
            }
            let pivot_row = a[c];
            for (r, row) in a.iter_mut().enumerate() {
                if r != c {
                    let factor = row[c];
                    for (value, pivot_value) in row.iter_mut().zip(pivot_row) {
                        *value -= factor * pivot_value;
                    }
                }
            }
        }
        let mut out = Self::IDENTITY;
        for (c, column) in out.cols.iter_mut().enumerate() {
            for (r, row) in a.iter().enumerate() {
                column[r] = row[c + 4] as f32;
            }
        }
        if !out.is_finite() {
            return None;
        }
        // Elimination can leave a tiny nonzero rounding pivot for an exactly
        // singular matrix. Validate the delivered f32 inverse in both directions
        // instead of declaring every finite candidate usable. This also rejects
        // ill-conditioned results whose identity residual exceeds 1e-3.
        for (left, right) in [(self, out), (out, self)] {
            for row in 0..4 {
                for column in 0..4 {
                    let value: f64 = (0..4)
                        .map(|k| left.cols[k][row] as f64 * right.cols[column][k] as f64)
                        .sum();
                    let expected = if row == column { 1. } else { 0. };
                    if !value.is_finite() || (value - expected).abs() > 1e-3 {
                        return None;
                    }
                }
            }
        }
        Some(out)
    }
    pub fn transform_vec4(self, v: [f32; 4]) -> [f32; 4] {
        let mut out = [0.; 4];
        for (r, value) in out.iter_mut().enumerate() {
            for (c, input) in v.iter().enumerate() {
                *value += self.cols[c][r] * input;
            }
        }
        out
    }
    pub fn transform_point3(self, v: Vec3) -> Vec3 {
        let p = self.transform_vec4([v.x, v.y, v.z, 1.]);
        Vec3::new(p[0] / p[3], p[1] / p[3], p[2] / p[3])
    }
    pub fn transform_vector3(self, v: Vec3) -> Vec3 {
        let p = self.transform_vec4([v.x, v.y, v.z, 0.]);
        Vec3::new(p[0], p[1], p[2])
    }
    pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Option<Self> {
        if ![fov_y, aspect, near, far].iter().all(|x| x.is_finite())
            || fov_y <= 0.
            || fov_y >= std::f32::consts::PI
            || aspect <= 0.
            || near <= 0.
            || far <= near
        {
            return None;
        }
        let f = 1. / (fov_y as f64 * 0.5).tan();
        let n = near as f64;
        let z = far as f64;
        let out = Self {
            cols: [
                [(f / aspect as f64) as f32, 0., 0., 0.],
                [0., f as f32, 0., 0.],
                [0., 0., (z / (n - z)) as f32, -1.],
                [0., 0., (n * z / (n - z)) as f32, 0.],
            ],
        };
        out.is_finite().then_some(out)
    }
    pub fn orthographic_rh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Option<Self> {
        if ![left, right, bottom, top, near, far]
            .iter()
            .all(|x| x.is_finite())
            || right <= left
            || top <= bottom
            || near < 0.
            || far <= near
        {
            return None;
        }
        let (left, right, bottom, top, near, far) = (
            left as f64,
            right as f64,
            bottom as f64,
            top as f64,
            near as f64,
            far as f64,
        );
        let out = Self {
            cols: [
                [(2. / (right - left)) as f32, 0., 0., 0.],
                [0., (2. / (top - bottom)) as f32, 0., 0.],
                [0., 0., (1. / (near - far)) as f32, 0.],
                [
                    (-(right + left) / (right - left)) as f32,
                    (-(top + bottom) / (top - bottom)) as f32,
                    (near / (near - far)) as f32,
                    1.,
                ],
            ],
        };
        out.is_finite().then_some(out)
    }
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Option<Self> {
        if !eye.is_finite() || !target.is_finite() || !up.is_finite() {
            return None;
        }
        let z = (eye - target).normalize_or_zero();
        let x = up.cross(z).normalize_or_zero();
        let y = z.cross(x);
        if z == Vec3::ZERO || x == Vec3::ZERO {
            return None;
        }
        let out = Self {
            cols: [
                [x.x, y.x, z.x, 0.],
                [x.y, y.y, z.y, 0.],
                [x.z, y.z, z.z, 0.],
                [-x.dot(eye), -y.dot(eye), -z.dot(eye), 1.],
            ],
        };
        out.is_finite().then_some(out)
    }
}
impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            cols: rhs.cols.map(|v| self.transform_vec4(v)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}
impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min.x <= max.x && min.y <= max.y && min.z <= max.z)
            .then_some(Self { min, max })
    }
    pub fn from_points(points: &[Vec3]) -> Option<Self> {
        let mut min = *points.first()?;
        let mut max = min;
        for p in points {
            if !p.is_finite() {
                return None;
            }
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
        Self::new(min, max)
    }
    pub fn contains(self, p: Vec3) -> bool {
        p.is_finite()
            && Self::new(self.min, self.max).is_some()
            && p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
    pub fn intersects(self, r: Self) -> bool {
        Self::new(self.min, self.max).is_some()
            && Self::new(r.min, r.max).is_some()
            && self.min.x <= r.max.x
            && self.max.x >= r.min.x
            && self.min.y <= r.max.y
            && self.max.y >= r.min.y
            && self.min.z <= r.max.z
            && self.max.z >= r.min.z
    }
    /// Bounds affine transforms. Projective bounds crossing w=0 are undefined.
    pub fn transformed(self, m: Mat4) -> Option<Self> {
        Self::new(self.min, self.max)?;
        if !m.is_finite()
            || m.cols[0][3] != 0.
            || m.cols[1][3] != 0.
            || m.cols[2][3] != 0.
            || m.cols[3][3] != 1.
        {
            return None;
        }
        let mut corners = [Vec3::ZERO; 8];
        for (i, p) in corners.iter_mut().enumerate() {
            *p = m.transform_point3(Vec3::new(
                if i & 1 == 0 { self.min.x } else { self.max.x },
                if i & 2 == 0 { self.min.y } else { self.max.y },
                if i & 4 == 0 { self.min.z } else { self.max.z },
            ));
        }
        Self::from_points(&corners)
    }
    /// Nonnegative ray parameters, preserving the magnitude of direction.
    pub fn ray_interval(self, ray: Ray) -> Option<(f32, f32)> {
        Self::new(self.min, self.max)?;
        if !ray.origin.is_finite() || !ray.direction.is_finite() || ray.direction == Vec3::ZERO {
            return None;
        }
        let mut lo = 0f64;
        let mut hi = f64::INFINITY;
        for (min, max, o, d) in [
            (self.min.x, self.max.x, ray.origin.x, ray.direction.x),
            (self.min.y, self.max.y, ray.origin.y, ray.direction.y),
            (self.min.z, self.max.z, ray.origin.z, ray.direction.z),
        ] {
            if d == 0. {
                if o < min || o > max {
                    return None;
                }
            } else {
                let a = (min as f64 - o as f64) / d as f64;
                let b = (max as f64 - o as f64) / d as f64;
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
                if lo > hi {
                    return None;
                }
            }
        }
        let out = (lo as f32, hi as f32);
        (out.0.is_finite() && out.1.is_finite()).then_some(out)
    }
}
