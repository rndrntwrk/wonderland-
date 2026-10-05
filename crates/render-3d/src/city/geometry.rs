use super::*;
use crate::legacy_random::LegacyRandom;
use wonderland_render_core::{RenderLimits, Vertex};

impl CityMap {
    pub fn validate(&self) -> Result<(), Error> {
        if self.width < 2
            || self.height < 2
            || self.width > 512
            || self.height > 512
            || self.pixels.len() != usize::from(self.width) * usize::from(self.height)
        {
            Err(Error::InvalidInput("city map dimensions/channels"))
        } else {
            Ok(())
        }
    }
    fn pixel(&self, x: i32, y: i32) -> CityPixel {
        let x = x.clamp(0, i32::from(self.width) - 1) as usize;
        let y = y.clamp(0, i32::from(self.height) - 1) as usize;
        self.pixels[y * usize::from(self.width) + x]
    }
    fn normal(&self, x: i32, y: i32) -> Vec3 {
        let x = x.clamp(0, i32::from(self.width) - 1);
        let y = y.clamp(0, i32::from(self.height) - 1);
        let h = |x, y| f32::from(self.pixel(x, y).elevation) / 6.;
        let mut n = Vec3::ZERO;
        if x < i32::from(self.width) - 1 {
            n = n + Vec3::new(h(x, y) - h(x + 1, y), 1., 0.);
        }
        if x > 1 {
            n = n + Vec3::new(h(x - 1, y) - h(x, y), 1., 0.);
        }
        if y < i32::from(self.height) - 1 {
            n = n + Vec3::new(0., 1., h(x, y) - h(x, y + 1));
        }
        if y > 1 {
            n = n + Vec3::new(0., 1., h(x, y - 1) - h(x, y));
        }
        let n = n.normalize_or_zero();
        if n == Vec3::ZERO {
            Vec3::Y
        } else {
            n
        }
    }
}
fn white_class(c: TerrainClass) -> [f32; 4] {
    match c {
        TerrainClass::Grass => [0., 1., 0., 1.],
        TerrainClass::Sand => [1., 1., 0., 1.],
        TerrainClass::Rock => [1., 0., 0., 1.],
        TerrainClass::Snow => [1.; 4],
        TerrainClass::Water => [12. / 255., 0., 1., 1.],
        TerrainClass::Void => [0.; 4],
    }
}
fn mesh_tile(map: &CityMap, x: i32, y: i32, color: [f32; 4]) -> Mesh {
    let vertices = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
        .into_iter()
        .map(|(xx, yy)| Vertex {
            position: Vec3::new(
                xx as f32,
                f32::from(map.pixel(xx, yy).elevation) / 12.,
                yy as f32,
            ),
            normal: map.normal(xx, yy),
            uv: Vec2::new(xx as f32 / 4., yy as f32 / 4.),
            color,
        })
        .collect();
    Mesh {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}
fn atlas_uv(mesh: &mut Mesh, index: u8, width: f32, height: f32) {
    let x = f32::from(index % (width as u8)) / width;
    let y = f32::from(index / (width as u8)) / height;
    for (v, uv) in mesh.vertices.iter_mut().zip([
        Vec2::new(x + 1. / width, y),
        Vec2::new(x, y),
        Vec2::new(x, y + 1. / height),
        Vec2::new(x + 1. / width, y + 1. / height),
    ]) {
        v.uv = uv;
    }
}
fn kind_order(k: CityPartKind) -> u8 {
    match k {
        CityPartKind::Terrain(c) | CityPartKind::Blend { class: c, .. } => c as u8,
        CityPartKind::RoadEdge(_) => 5,
        CityPartKind::RoadCorner(_) => 6,
    }
}
pub fn build_city_parts(
    map: &CityMap,
    boundary: CityBoundary,
    max_vertices: usize,
) -> Result<Vec<CityPart>, Error> {
    map.validate()?;
    if !matches!(boundary, CityBoundary::Rectangle) && (map.width != 512 || map.height != 512) {
        return Err(Error::InvalidInput(
            "legacy city boundaries require 512x512 channels",
        ));
    }
    let mut parts = Vec::new();
    let mut used = 0usize;
    let mut add = |x: u16, y: u16, kind, mesh: Mesh| -> Result<(), Error> {
        used += mesh.vertices.len();
        if used > max_vertices || used > 2000000 {
            return Err(Error::BudgetExceeded("city vertices"));
        }
        parts.push(CityPart {
            tile: (x, y),
            chunk: (x / 16, y / 16),
            kind,
            mesh,
        });
        Ok(())
    };
    for y in 0..map.height {
        for x in 0..map.width {
            if !in_bounds(i32::from(x), i32::from(y), boundary) {
                continue;
            }
            let pixel = map.pixel(i32::from(x), i32::from(y));
            let class = terrain_class(pixel.terrain);
            if class == TerrainClass::Void {
                continue;
            }
            let mesh = mesh_tile(map, i32::from(x), i32::from(y), white_class(class));
            add(x, y, CityPartKind::Terrain(class), mesh.clone())?;
            let mut absence = 0;
            let mut next = TerrainClass::Water;
            for (dx, dy, bit) in [(0, -1, 2), (1, 0, 1), (0, 1, 8), (-1, 0, 4)] {
                let xx = i32::from(x) + dx;
                let yy = i32::from(y) + dy;
                let c = terrain_class(map.pixel(xx, yy).terrain);
                if xx < 0
                    || yy < 0
                    || xx >= i32::from(map.width)
                    || yy >= i32::from(map.height)
                    || c == TerrainClass::Void
                    || (c as u8) <= (class as u8)
                {
                    absence |= bit;
                } else if (c as u8) < (next as u8) {
                    next = c;
                }
            }
            if absence < 15 {
                let mut blend = mesh.clone();
                for v in &mut blend.vertices {
                    v.color = white_class(next);
                }
                atlas_uv(&mut blend, blend_atlas(absence), 7., 3.);
                add(
                    x,
                    y,
                    CityPartKind::Blend {
                        class: next,
                        mask: absence,
                    },
                    blend,
                )?;
            }
            let (edge, corner) = road_atlas(pixel.road);
            for (kind, index) in [
                (edge.map(CityPartKind::RoadEdge), edge),
                (corner.map(CityPartKind::RoadCorner), corner),
            ] {
                if let (Some(kind), Some(index)) = (kind, index) {
                    let mut road = mesh.clone();
                    atlas_uv(&mut road, index, 8., 4.);
                    for v in &mut road.vertices {
                        v.color = [0.25, 0.25, 0.25, 1.];
                    }
                    add(x, y, kind, road)?;
                }
            }
        }
    }
    parts.sort_by_key(|p| (kind_order(p.kind), p.chunk.1, p.chunk.0, p.tile.1, p.tile.0));
    Ok(parts)
}
pub fn build_city_mesh(map: &CityMap, boundary: CityBoundary) -> Result<Mesh, Error> {
    let parts = build_city_parts(map, boundary, 2000000)?;
    let mut result = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for p in parts {
        let base = result.vertices.len() as u32;
        result
            .indices
            .extend(p.mesh.indices.into_iter().map(|i| i + base));
        result.vertices.extend(p.mesh.vertices);
    }
    result
        .validate(&RenderLimits::default())
        .map_err(|_| Error::InvalidInput("city mesh"))?;
    Ok(result)
}
fn height_cubic(
    map: &CityMap,
    x: i32,
    y: i32,
    u: f32,
    v: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> f32 {
    let mut columns = [0.; 4];
    for (i, c) in columns.iter_mut().enumerate() {
        let values =
            [-1, 0, 1, 2].map(|dy| f32::from(map.pixel(x + i as i32 - 1, y + dy).elevation));
        *c = cubic(values, v, if i < 2 { left } else { right });
    }
    cubic(columns, u, top * (1. - v) + bottom * v) / 12.
}
pub fn build_near_patch(
    map: &CityMap,
    origin: (u16, u16),
    size: (u16, u16),
    subdiv: u8,
) -> Result<Mesh, Error> {
    map.validate()?;
    if subdiv == 0
        || subdiv > 8
        || size.0 == 0
        || size.1 == 0
        || size.0 > 32
        || size.1 > 32
        || u32::from(origin.0) + u32::from(size.0) > u32::from(map.width)
        || u32::from(origin.1) + u32::from(size.1) > u32::from(map.height)
    {
        return Err(Error::InvalidInput("near patch extent/subdivision"));
    }
    let n = usize::from(subdiv) + 1;
    let mut mesh = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for ty in 0..size.1 {
        for tx in 0..size.0 {
            let x = i32::from(origin.0 + tx);
            let y = i32::from(origin.1 + ty);
            let c = terrain_class(map.pixel(x, y).terrain);
            if c == TerrainClass::Void {
                continue;
            }
            let base = mesh.vertices.len() as u32;
            let left = if tx == 0 { -1. } else { 0. };
            let right = if tx == size.0 - 1 { -1. } else { 0. };
            let top = if ty == 0 { -1. } else { 0. };
            let bottom = if ty == size.1 - 1 { -1. } else { 0. };
            for iy in 0..n {
                for ix in 0..n {
                    let u = ix as f32 / f32::from(subdiv);
                    let v = iy as f32 / f32::from(subdiv);
                    let h = height_cubic(map, x, y, u, v, left, right, top, bottom);
                    let normal = map
                        .normal(x, y)
                        .lerp(map.normal(x + 1, y), u)
                        .lerp(map.normal(x, y + 1).lerp(map.normal(x + 1, y + 1), u), v);
                    mesh.vertices.push(Vertex {
                        position: Vec3::new(x as f32 + u, h, y as f32 + v),
                        normal,
                        uv: Vec2::new((x as f32 + u) / 4., (y as f32 + v) / 4.),
                        color: white_class(c),
                    });
                }
            }
            for iy in 0..n - 1 {
                for ix in 0..n - 1 {
                    let a = base + (iy * n + ix) as u32;
                    mesh.indices.extend([
                        a,
                        a + 1,
                        a + n as u32 + 1,
                        a,
                        a + n as u32 + 1,
                        a + n as u32,
                    ]);
                }
            }
        }
    }
    mesh.validate(&RenderLimits::default())
        .map_err(|_| Error::InvalidInput("city near mesh"))?;
    Ok(mesh)
}
pub fn foliage_instances(map: &CityMap, x: u16, y: u16) -> Result<Vec<FoliageInstance>, Error> {
    map.validate()?;
    if x >= map.width || y >= map.height {
        return Err(Error::InvalidInput("foliage location"));
    }
    let p = map.pixel(i32::from(x), i32::from(y));
    let class = forest_class(p.forest);
    let Some(mut ty) = class.atlas_3d() else {
        return Ok(Vec::new());
    };
    if ty == 0 && terrain_class(p.terrain) == TerrainClass::Snow {
        ty = 4;
    }
    let mut rng = LegacyRandom::new(i32::from(y) * 512 + i32::from(x));
    let mut out = Vec::new();
    let (mut sx, mut sy, mut rx, mut ry) = (0., 0., 1., 1.);
    if p.road & 1 != 0 {
        sx += 0.15;
        rx -= 0.15;
    }
    if p.road & 2 != 0 {
        ry -= 0.15;
    }
    if p.road & 4 != 0 {
        rx -= 0.15;
    }
    if p.road & 8 != 0 {
        sy += 0.15;
        ry -= 0.15;
    }
    for _ in 0..tree_count(p.density) {
        let model = (ty * 4).min(15) + rng.next(if ty >= 3 { 3 } else { 4 }) as u8;
        let u = rng.unit() as f32 * rx + sx;
        let v = rng.unit() as f32 * ry + sy;
        let h = height_cubic(map, i32::from(x), i32::from(y), u, v, 0., 0., 0., 0.);
        let yaw = (std::f64::consts::TAU * rng.unit()) as f32;
        out.push(FoliageInstance {
            model,
            position: Vec3::new(f32::from(x) + u, h, f32::from(y) + v),
            yaw,
            scale: 1. / 75.,
        });
    }
    Ok(out)
}
