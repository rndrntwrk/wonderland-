//! LMapBatch.GetLightMat/GetSunlightMat/Draw3DObjShadows and RCObject.psLMapRC.
//! Inputs are effective visible object triangles after source group/mask selection,
//! in graphics units relative to the target floor (container offsets already applied).
use super::{LightingBudget, ShadowLight, ShadowLightKind};
use crate::{IsoError, Result};
use wonderland_render_core::{cache::DerivedKey, AssetKey, RgbaImage, Vec2, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct MeshShadowInput {
    pub source: AssetKey,
    pub vertices: Vec<Vec3>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RoomMeshShadows {
    pub room: u16,
    pub floor: u8,
    pub meshes: Vec<MeshShadowInput>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedShadowVertex {
    pub position: Vec2,
    pub attenuation: f32,
    pub reciprocal_w: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedShadowMesh {
    key: DerivedKey,
    vertices: Vec<ProjectedShadowVertex>,
    indices: Vec<u32>,
}
impl ProjectedShadowMesh {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn vertices(&self) -> &[ProjectedShadowVertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn resident_bytes(&self) -> usize {
        self.vertices.capacity() * std::mem::size_of::<ProjectedShadowVertex>()
            + self.indices.capacity() * std::mem::size_of::<u32>()
    }
}
pub(super) fn validate_mesh(input: &MeshShadowInput, budget: LightingBudget) -> Result<()> {
    if input.vertices.len() > budget.max_vertices
        || input.indices.len() > budget.max_indices
        || input.vertices.len() > u32::MAX as usize
    {
        return Err(IsoError::Limit("object shadow mesh input"));
    }
    if input.indices.len() % 3 != 0
        || input
            .indices
            .iter()
            .any(|&i| i as usize >= input.vertices.len())
        || input.vertices.iter().any(|p| {
            !p.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 10000. || p.z.abs() > 1_000_000.
        })
    {
        return Err(IsoError::Invalid("object shadow mesh input"));
    }
    Ok(())
}
pub fn project_object_shadow(
    input: &MeshShadowInput,
    light: ShadowLight,
    height_tiles: f32,
    budget: LightingBudget,
) -> Result<ProjectedShadowMesh> {
    light.validate()?;
    validate_mesh(input, budget)?;
    if !height_tiles.is_finite() || !(0.01..=1000.).contains(&height_tiles) {
        return Err(IsoError::Invalid("object shadow light height"));
    }
    // Polygon clipping and its temporary vectors are owned during projection.
    budget.bytes(512)?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for tri in input.indices.chunks_exact(3) {
        let mut polygon = vec![
            input.vertices[tri[0] as usize],
            input.vertices[tri[1] as usize],
            input.vertices[tri[2] as usize],
        ];
        // The point frustum far plane is the floor, its near plane is 0.01 tile below the light.
        if light.kind == ShadowLightKind::Point {
            polygon = clip_height(&polygon, 0., true);
            polygon = clip_height(&polygon, (height_tiles - 0.01) * 3., false);
        }
        if polygon.len() < 3 {
            continue;
        }
        let v = vertices
            .len()
            .checked_add(polygon.len())
            .ok_or(IsoError::Limit("projected shadow vertices"))?;
        let n = indices
            .len()
            .checked_add((polygon.len() - 2) * 3)
            .ok_or(IsoError::Limit("projected shadow indices"))?;
        if v > budget.max_vertices || n > budget.max_indices || v > u32::MAX as usize {
            return Err(IsoError::Limit("projected shadow geometry"));
        }
        budget.bytes(
            v.checked_mul(64)
                .and_then(|x| x.checked_add(n.checked_mul(16)?))
                .and_then(|x| x.checked_add(512))
                .ok_or(IsoError::Limit("projected shadow bytes"))?,
        )?;
        let base = vertices.len() as u32;
        for p in polygon {
            let ground = Vec2::new(p.x / 3. * 16., p.z / 3. * 16.);
            let (position, reciprocal_w) = if light.kind == ShadowLightKind::Directional {
                (
                    ground + light.direction * (p.y / 9. * 32. * light.falloff_multiplier),
                    1.,
                )
            } else {
                let w = height_tiles - p.y / 3.;
                let factor = height_tiles / w;
                (
                    light.position_sixteenths + (ground - light.position_sixteenths) * factor,
                    1. / w,
                )
            };
            let attenuation = 1. - p.y / (3. * 2.95 * 5.);
            if !position.is_finite() || !reciprocal_w.is_finite() || !attenuation.is_finite() {
                return Err(IsoError::Invalid("object shadow projection overflow"));
            }
            vertices.push(ProjectedShadowVertex {
                position,
                attenuation,
                reciprocal_w,
            });
        }
        for i in 1..(v - base as usize - 1) {
            indices.extend_from_slice(&[base, base + i as u32, base + i as u32 + 1]);
        }
    }
    let key_capacity = b"source-projected-object-shadow-v1\0".len()
        + 32
        + 16
        + vertices.len() * 16
        + indices.len() * 4;
    budget.bytes(vertices.capacity() * 16 + indices.capacity() * 4 + key_capacity)?;
    let mut bytes = Vec::with_capacity(key_capacity);
    bytes.extend_from_slice(b"source-projected-object-shadow-v1\0");
    bytes.extend_from_slice(&input.source.0);
    bytes.extend_from_slice(&(vertices.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(indices.len() as u64).to_le_bytes());
    for v in &vertices {
        for f in [v.position.x, v.position.y, v.attenuation, v.reciprocal_w] {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }
    for i in &indices {
        bytes.extend_from_slice(&i.to_le_bytes());
    }
    let key = DerivedKey::new(input.source, input.source, 1, &bytes);
    Ok(ProjectedShadowMesh {
        key,
        vertices,
        indices,
    })
}
fn clip_height(polygon: &[Vec3], plane: f32, above: bool) -> Vec<Vec3> {
    let mut result = vec![];
    if polygon.is_empty() {
        return result;
    }
    let inside = |p: Vec3| if above { p.y >= plane } else { p.y <= plane };
    let mut previous = *polygon.last().unwrap();
    let mut previous_inside = inside(previous);
    for &current in polygon {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            let t = (plane - previous.y) / (current.y - previous.y);
            result.push(previous + (current - previous) * t);
        }
        if current_inside {
            result.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    result
}
pub fn rasterize_projected_shadow(
    mesh: &ProjectedShadowMesh,
    size: [u32; 2],
    extent: Vec2,
    budget: LightingBudget,
) -> Result<RgbaImage> {
    rasterize_projected_counted(mesh, size, extent, budget).map(|v| v.0)
}
pub(super) fn rasterize_projected_counted(
    mesh: &ProjectedShadowMesh,
    size: [u32; 2],
    extent: Vec2,
    budget: LightingBudget,
) -> Result<(RgbaImage, u64)> {
    let count = budget.texture(size[0], size[1])?;
    if !extent.is_finite()
        || extent.x <= 0.
        || extent.y <= 0.
        || extent.x > 1_000_000.
        || extent.y > 1_000_000.
    {
        return Err(IsoError::Invalid("projected shadow target extent"));
    }
    budget.bytes(
        count
            .checked_mul(4)
            .and_then(|n| n.checked_add(mesh.resident_bytes()))
            .and_then(|n| n.checked_add(mesh.indices.len() / 3 * 16))
            .ok_or(IsoError::Limit("projected target bytes"))?,
    )?;
    let scale = Vec2::new(size[0] as f32 / extent.x, size[1] as f32 / extent.y);
    let mut ranges = Vec::with_capacity(mesh.indices.len() / 3);
    let mut work = count as u64;
    if work > budget.max_raster_samples {
        return Err(IsoError::Limit("projected shadow target clear work"));
    }
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize].position);
        let range = [
            (a.x.min(b.x).min(c.x) * scale.x)
                .floor()
                .clamp(0., size[0] as f32) as u32,
            (a.y.min(b.y).min(c.y) * scale.y)
                .floor()
                .clamp(0., size[1] as f32) as u32,
            (a.x.max(b.x).max(c.x) * scale.x)
                .ceil()
                .clamp(0., size[0] as f32) as u32,
            (a.y.max(b.y).max(c.y) * scale.y)
                .ceil()
                .clamp(0., size[1] as f32) as u32,
        ];
        work = work
            .checked_add(u64::from(range[2] - range[0]) * u64::from(range[3] - range[1]))
            .ok_or(IsoError::Limit("projected raster work"))?;
        if work > budget.max_raster_samples {
            return Err(IsoError::Limit("projected raster work"));
        }
        ranges.push(range);
    }
    let mut pixels = vec![[0, 0, 0, 255]; count];
    for (tri, r) in mesh.indices.chunks_exact(3).zip(ranges) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize]);
        let area = edge(a.position, b.position, c.position);
        if area == 0. {
            continue;
        }
        for y in r[1]..r[3] {
            for x in r[0]..r[2] {
                let p = Vec2::new((x as f32 + 0.5) / scale.x, (y as f32 + 0.5) / scale.y);
                let weights = [
                    edge(b.position, c.position, p) / area,
                    edge(c.position, a.position, p) / area,
                    edge(a.position, b.position, p) / area,
                ];
                if weights.iter().any(|w| *w < 0.) {
                    continue;
                }
                let weights = [
                    weights[0] * a.reciprocal_w,
                    weights[1] * b.reciprocal_w,
                    weights[2] * c.reciprocal_w,
                ];
                let sum = weights.iter().sum::<f32>();
                let intensity = ((a.attenuation * weights[0]
                    + b.attenuation * weights[1]
                    + c.attenuation * weights[2])
                    / sum)
                    .clamp(0., 1.);
                if !intensity.is_finite() {
                    return Err(IsoError::Invalid("projected shadow interpolation"));
                }
                let out = &mut pixels[(y * size[0] + x) as usize][1];
                *out = (*out).max((intensity * 255.).round() as u8);
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
fn edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
