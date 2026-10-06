//! Bounded port of tso.common/MeshSimplify (Fast Quadratic Mesh Simplification).
//! Source order: initialize quadrics/errors, build references, mark borders;
//! every fifth iteration compacts references, and every accepted edge updates
//! adjacent triangle errors. The active reconstruction schedule is 125/3.5/100.
use crate::Error;
use std::collections::BTreeMap;
use wonderland_render_core::{Mesh, RenderLimits, Vec3, Vertex};

// MonoGame Vector3.Length rounds the sum of squared components to f32, then
// computes Math.Sqrt in f64 and rounds the length back to f32. Normalize and
// scalar division multiply by an f32 reciprocal. These intermediate roundings
// affect quadrics and the collapse order on curved surfaces, so source-sensitive
// steps use local helpers instead of the core's widened, robust vector math.
fn source_length(vector: Vec3) -> f32 {
    f64::from(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt() as f32
}

fn source_normalized(vector: Vec3) -> Vec3 {
    let length = source_length(vector);
    if length == 0. || !length.is_finite() {
        Vec3::ZERO
    } else {
        vector * (1. / length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimplificationOptions {
    pub target_triangles: usize,
    pub iterations: u16,
    pub aggressiveness: f64,
    /// Bounded vertex/triangle/reference visits; fixed-size matrix arithmetic is
    /// charged with its enclosing visit. This is an operation cap, not a timer.
    pub max_operations: usize,
    pub max_reference_entries: usize,
}
impl SimplificationOptions {
    pub fn source(triangles: usize) -> Self {
        Self {
            target_triangles: triangles / 100,
            iterations: 125,
            aggressiveness: 3.5,
            max_operations: 100_000_000,
            max_reference_entries: 8_000_000,
        }
    }
}
#[derive(Clone, Debug)]
pub struct SimplificationOutput {
    pub mesh: Mesh,
    pub original_triangles: usize,
    pub remaining_triangles: usize,
    pub iterations_used: u16,
    pub target_reached: bool,
    pub operations: usize,
}
#[derive(Clone, Copy, Debug, Default)]
struct Quadric([f64; 10]);
impl Quadric {
    fn plane(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self([
            a * a,
            a * b,
            a * c,
            a * d,
            b * b,
            b * c,
            b * d,
            c * c,
            c * d,
            d * d,
        ])
    }
    fn det(self, i: [usize; 9]) -> f64 {
        let [a, b, c, d, e, f, g, h, j] = i;
        let m = self.0;
        m[a] * m[e] * m[j] + m[c] * m[d] * m[h] + m[b] * m[f] * m[g]
            - m[c] * m[e] * m[g]
            - m[a] * m[f] * m[h]
            - m[b] * m[d] * m[j]
    }
    fn error(self, p: Vec3) -> f64 {
        let (x, y, z) = (f64::from(p.x), f64::from(p.y), f64::from(p.z));
        let q = self.0;
        q[0] * x * x
            + 2. * q[1] * x * y
            + 2. * q[2] * x * z
            + 2. * q[3] * x
            + q[4] * y * y
            + 2. * q[5] * y * z
            + 2. * q[6] * y
            + q[7] * z * z
            + 2. * q[8] * z
            + q[9]
    }
}
impl std::ops::Add for Quadric {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }
}
#[derive(Clone)]
struct QVertex {
    value: Vertex,
    quadric: Quadric,
    start: usize,
    count: usize,
    border: bool,
}
#[derive(Clone, Copy)]
struct Triangle {
    vertices: [usize; 3],
    errors: [f64; 4],
    deleted: bool,
    dirty: bool,
    normal: Vec3,
}
#[derive(Clone, Copy, Default)]
struct Reference {
    triangle: usize,
    corner: usize,
}
struct State {
    vertices: Vec<QVertex>,
    triangles: Vec<Triangle>,
    references: Vec<Reference>,
    options: SimplificationOptions,
    operations: usize,
}
impl State {
    fn charge(&mut self, amount: usize) -> Result<(), Error> {
        let used = self
            .operations
            .checked_add(amount)
            .ok_or(Error::BudgetExceeded("simplification operations"))?;
        if used > self.options.max_operations {
            return Err(Error::BudgetExceeded("simplification operations"));
        }
        self.operations = used;
        Ok(())
    }
    fn edge_error(&self, first: usize, second: usize) -> Result<(f64, Vec3), Error> {
        let a = &self.vertices[first];
        let b = &self.vertices[second];
        let q = a.quadric + b.quadric;
        let determinant = q.det([0, 1, 2, 1, 4, 5, 2, 5, 7]);
        let (error, p) = if determinant != 0. && !(a.border && b.border) {
            let p = Vec3::new(
                ((-1. / determinant) * q.det([1, 2, 3, 4, 5, 6, 5, 7, 8])) as f32,
                ((1. / determinant) * q.det([0, 2, 3, 1, 5, 6, 2, 7, 8])) as f32,
                ((-1. / determinant) * q.det([0, 1, 3, 1, 4, 6, 2, 5, 8])) as f32,
            );
            (q.error(p), p)
        } else {
            let p1 = a.value.position;
            let p2 = b.value.position;
            let mut p3 = (p1 + p2) / 2.;
            if !p3.is_finite() {
                p3 = Vec3::new(
                    ((f64::from(p1.x) + f64::from(p2.x)) / 2.) as f32,
                    ((f64::from(p1.y) + f64::from(p2.y)) / 2.) as f32,
                    ((f64::from(p1.z) + f64::from(p2.z)) / 2.) as f32,
                );
            }
            let e1 = q.error(p1);
            let e2 = q.error(p2);
            let e3 = q.error(p3);
            let error = e1.min(e2.min(e3));
            // Preserve the source tie order: midpoint wins a three-way tie.
            let mut result = p1;
            if e2 == error {
                result = p2;
            }
            if e3 == error {
                result = p3;
            }
            (error, result)
        };
        if !p.is_finite() || !error.is_finite() {
            return Err(Error::InvalidInput("nonfinite quadric collapse"));
        }
        Ok((error, p))
    }
    fn update_errors(&mut self, triangle: usize) -> Result<(), Error> {
        self.charge(3)?;
        let ids = self.triangles[triangle].vertices;
        let a = self.edge_error(ids[0], ids[1])?.0;
        let b = self.edge_error(ids[1], ids[2])?.0;
        let c = self.edge_error(ids[2], ids[0])?.0;
        self.triangles[triangle].errors = [a, b, c, a.min(b.min(c))];
        Ok(())
    }
    fn update_mesh(&mut self, iteration: u16) -> Result<(), Error> {
        self.charge(self.triangles.len() + self.vertices.len())?;
        if iteration > 0 {
            self.triangles.retain(|t| !t.deleted);
        } else {
            for v in &mut self.vertices {
                v.quadric = Quadric::default();
            }
            for i in 0..self.triangles.len() {
                self.charge(3)?;
                let ids = self.triangles[i].vertices;
                let p = ids.map(|id| self.vertices[id].value.position);
                let n = source_normalized((p[1] - p[0]).cross(p[2] - p[0]));
                if n == Vec3::ZERO {
                    return Err(Error::InvalidInput("degenerate simplification triangle"));
                }
                self.triangles[i].normal = n;
                let q = Quadric::plane(
                    f64::from(n.x),
                    f64::from(n.y),
                    f64::from(n.z),
                    f64::from(-n.dot(p[0])),
                );
                for id in ids {
                    self.vertices[id].quadric = self.vertices[id].quadric + q;
                }
            }
            // This intentionally occurs before initial border classification.
            for i in 0..self.triangles.len() {
                self.update_errors(i)?;
            }
        }
        for v in &mut self.vertices {
            v.start = 0;
            v.count = 0;
        }
        for t in &self.triangles {
            for id in t.vertices {
                self.vertices[id].count += 1;
            }
        }
        let count = self
            .triangles
            .len()
            .checked_mul(3)
            .ok_or(Error::BudgetExceeded("simplification references"))?;
        if count > self.options.max_reference_entries {
            return Err(Error::BudgetExceeded("simplification references"));
        }
        self.charge(count)?;
        let mut start = 0usize;
        for v in &mut self.vertices {
            v.start = start;
            start += v.count;
            v.count = 0;
        }
        self.references.clear();
        self.references.resize(count, Reference::default());
        for (i, triangle) in self.triangles.iter().enumerate() {
            for (corner, id) in triangle.vertices.into_iter().enumerate() {
                let v = &mut self.vertices[id];
                self.references[v.start + v.count] = Reference {
                    triangle: i,
                    corner,
                };
                v.count += 1;
            }
        }
        if iteration == 0 {
            for v in &mut self.vertices {
                v.border = false;
            }
            for i in 0..self.vertices.len() {
                let (start, count) = (self.vertices[i].start, self.vertices[i].count);
                let mut neighbors = BTreeMap::<usize, usize>::new();
                for k in 0..count {
                    self.charge(3)?;
                    let r = self.references[start + k];
                    for id in self.triangles[r.triangle].vertices {
                        *neighbors.entry(id).or_default() += 1;
                    }
                }
                // Counting in a map preserves the source count==1 predicate
                // while avoiding its quadratic search at a high-valence vertex.
                for (id, count) in neighbors {
                    if count == 1 {
                        self.vertices[id].border = true;
                    }
                }
            }
        }
        Ok(())
    }
    fn flipped(
        &mut self,
        position: Vec3,
        other: usize,
        start: usize,
        count: usize,
    ) -> Result<Option<Vec<bool>>, Error> {
        let mut deleted = vec![false; count];
        for (k, deletion) in deleted.iter_mut().enumerate() {
            self.charge(1)?;
            let r = self.references[start + k];
            let triangle = self.triangles[r.triangle];
            if triangle.deleted {
                continue;
            }
            let id1 = triangle.vertices[(r.corner + 1) % 3];
            let id2 = triangle.vertices[(r.corner + 2) % 3];
            if id1 == other || id2 == other {
                *deletion = true;
                continue;
            }
            let d1 = source_normalized(self.vertices[id1].value.position - position);
            let d2 = source_normalized(self.vertices[id2].value.position - position);
            if d1 == Vec3::ZERO || d2 == Vec3::ZERO || d1.dot(d2).abs() > 0.999 {
                return Ok(None);
            }
            let normal = source_normalized(d1.cross(d2));
            if normal == Vec3::ZERO || normal.dot(triangle.normal) < 0.2 {
                return Ok(None);
            }
        }
        Ok(Some(deleted))
    }
    fn update_triangles(
        &mut self,
        first: usize,
        start: usize,
        count: usize,
        deleted: &[bool],
        deleted_count: &mut usize,
    ) -> Result<(), Error> {
        for k in 0..count {
            self.charge(1)?;
            let r = self.references[start + k];
            if self.triangles[r.triangle].deleted {
                continue;
            }
            if deleted[k] {
                self.triangles[r.triangle].deleted = true;
                *deleted_count += 1;
                continue;
            }
            self.triangles[r.triangle].vertices[r.corner] = first;
            self.triangles[r.triangle].dirty = true;
            self.update_errors(r.triangle)?;
            if self.references.len() >= self.options.max_reference_entries {
                return Err(Error::BudgetExceeded("simplification references"));
            }
            self.references.push(r);
        }
        Ok(())
    }
    fn compact(&mut self) -> Result<Mesh, Error> {
        self.charge(self.triangles.len() + self.vertices.len())?;
        let mut used = vec![false; self.vertices.len()];
        for t in self.triangles.iter().filter(|t| !t.deleted) {
            for id in t.vertices {
                used[id] = true;
            }
        }
        let mut remap = vec![0u32; self.vertices.len()];
        let mut vertices = Vec::new();
        for (i, vertex) in self.vertices.iter().enumerate() {
            if used[i] {
                remap[i] = vertices.len() as u32;
                let mut value = vertex.value.clone();
                value.normal = Vec3::ZERO;
                vertices.push(value);
            }
        }
        let mut indices = Vec::new();
        for t in self.triangles.iter().filter(|t| !t.deleted) {
            let ids = t.vertices.map(|id| remap[id]);
            indices.extend(ids);
            let p = ids.map(|id| vertices[id as usize].position);
            let normal = (p[1] - p[0]).cross(p[2] - p[0]);
            if !normal.is_finite() {
                return Err(Error::InvalidInput("simplification normal overflow"));
            }
            for id in ids {
                vertices[id as usize].normal = vertices[id as usize].normal + normal;
            }
        }
        for v in &mut vertices {
            v.normal = v.normal.normalize_or_zero();
        }
        let mesh = Mesh { vertices, indices };
        mesh.validate(&RenderLimits::default())
            .map_err(|_| Error::InvalidInput("simplified mesh"))?;
        Ok(mesh)
    }
}
/// The caller retains its immutable input on all errors. Geometry uses the
/// source f32 positions/f64 quadrics; source nonfinite or degenerate cases are
/// rejected explicitly. UV/color interpolation is retained for generic meshes;
/// reconstruction subsequently applies the source inverse-camera UV projection.
pub fn simplify_mesh(
    mesh: &Mesh,
    options: SimplificationOptions,
) -> Result<SimplificationOutput, Error> {
    if options.iterations > 500
        || !options.aggressiveness.is_finite()
        || options.aggressiveness < 0.
        || options.aggressiveness > 16.
    {
        return Err(Error::InvalidInput("simplification schedule"));
    }
    mesh.validate(&RenderLimits::default())
        .map_err(|_| Error::InvalidInput("simplification mesh"))?;
    let original_triangles = mesh.indices.len() / 3;
    let mut state = State {
        vertices: mesh
            .vertices
            .iter()
            .cloned()
            .map(|value| QVertex {
                value,
                quadric: Quadric::default(),
                start: 0,
                count: 0,
                border: false,
            })
            .collect(),
        triangles: mesh
            .indices
            .chunks_exact(3)
            .map(|t| Triangle {
                vertices: [t[0] as usize, t[1] as usize, t[2] as usize],
                errors: [0.; 4],
                deleted: false,
                dirty: false,
                normal: Vec3::ZERO,
            })
            .collect(),
        references: Vec::new(),
        options,
        operations: 0,
    };
    state.charge(mesh.vertices.len() + mesh.indices.len())?;
    let mut deleted_count = 0usize;
    let mut iterations_used = 0u16;
    for iteration in 0..options.iterations {
        if original_triangles - deleted_count <= options.target_triangles {
            break;
        }
        iterations_used = iteration + 1;
        if iteration % 5 == 0 {
            state.update_mesh(iteration)?;
        }
        state.charge(state.triangles.len())?;
        for t in &mut state.triangles {
            t.dirty = false;
        }
        let threshold = 0.000000001 * (f64::from(iteration) + 3.).powf(options.aggressiveness);
        for i in 0..state.triangles.len() {
            state.charge(1)?;
            let triangle = state.triangles[i];
            if triangle.errors[3] > threshold || triangle.deleted || triangle.dirty {
                continue;
            }
            for edge in 0..3 {
                if triangle.errors[edge] >= threshold {
                    continue;
                }
                let first = triangle.vertices[edge];
                let second = triangle.vertices[(edge + 1) % 3];
                if state.vertices[first].border != state.vertices[second].border {
                    continue;
                }
                state.charge(1)?;
                let (_, position) = state.edge_error(first, second)?;
                let (start0, count0) = (state.vertices[first].start, state.vertices[first].count);
                let (start1, count1) = (state.vertices[second].start, state.vertices[second].count);
                let Some(deleted0) = state.flipped(position, second, start0, count0)? else {
                    continue;
                };
                let Some(deleted1) = state.flipped(position, first, start1, count1)? else {
                    continue;
                };
                let old0 = state.vertices[first].value.clone();
                let old1 = state.vertices[second].value.clone();
                let direction = old1.position - old0.position;
                let length = source_length(direction);
                // Degenerate source normalization would produce NaNs. Such an
                // edge has no usable texture interpolation and is not collapsed.
                if length == 0. || !length.is_finite() {
                    continue;
                }
                let reciprocal = 1. / length;
                let factor = (direction * reciprocal).dot((position - old0.position) * reciprocal);
                if !factor.is_finite() {
                    continue;
                }
                let value = &mut state.vertices[first].value;
                value.position = position;
                // MonoGame MathHelper.Lerp, also used by Vector2.Lerp.
                value.uv = old0.uv + (old1.uv - old0.uv) * factor;
                value.color =
                    std::array::from_fn(|c| old0.color[c] * (1. - factor) + old1.color[c] * factor);
                state.vertices[first].quadric =
                    state.vertices[second].quadric + state.vertices[first].quadric;
                let new_start = state.references.len();
                state.update_triangles(first, start0, count0, &deleted0, &mut deleted_count)?;
                state.update_triangles(first, start1, count1, &deleted1, &mut deleted_count)?;
                let new_count = state.references.len() - new_start;
                if new_count <= count0 {
                    for k in 0..new_count {
                        state.references[start0 + k] = state.references[new_start + k];
                    }
                } else {
                    state.vertices[first].start = new_start;
                }
                state.vertices[first].count = new_count;
                break;
            }
            if original_triangles - deleted_count <= options.target_triangles {
                break;
            }
        }
    }
    let mesh = state.compact()?;
    let remaining_triangles = mesh.indices.len() / 3;
    Ok(SimplificationOutput {
        mesh,
        original_triangles,
        remaining_triangles,
        iterations_used,
        target_reached: remaining_triangles <= options.target_triangles,
        operations: state.operations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_render_core::Vec2;
    #[test]
    fn quadric_determinants_recover_the_three_plane_intersection() {
        let q = Quadric::plane(1., 0., 0., -2.)
            + Quadric::plane(0., 1., 0., -3.)
            + Quadric::plane(0., 0., 1., -4.);
        let vertex = |quadric| QVertex {
            value: Vertex {
                position: Vec3::ZERO,
                normal: Vec3::ZERO,
                uv: Vec2::ZERO,
                color: [1.; 4],
            },
            quadric,
            start: 0,
            count: 0,
            border: false,
        };
        let state = State {
            vertices: vec![vertex(q), vertex(Quadric::default())],
            triangles: Vec::new(),
            references: Vec::new(),
            options: SimplificationOptions::source(0),
            operations: 0,
        };
        let (error, p) = state.edge_error(0, 1).unwrap();
        assert_eq!(p, Vec3::new(2., 3., 4.));
        assert_eq!(error, 0.);
        assert_eq!(q.error(Vec3::new(3., 3., 4.)), 1.);
    }
}
