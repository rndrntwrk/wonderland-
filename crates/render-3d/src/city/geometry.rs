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

fn renderer_boundary(boundary: CityBoundary) -> bool {
    matches!(
        boundary,
        CityBoundary::RendererDiamond | CityBoundary::RendererWithFade
    )
}
fn row_limits(y: i32) -> (i32, i32) {
    ((y - 306).abs(), if y < 205 { 307 + y } else { 717 - y })
}
fn validate_boundary(map: &CityMap, boundary: CityBoundary) -> Result<(), Error> {
    if !matches!(boundary, CityBoundary::Rectangle) && (map.width != 512 || map.height != 512) {
        return Err(Error::InvalidInput(
            "legacy city boundaries require 512x512 channels",
        ));
    }
    Ok(())
}
fn sample_x(x: i32, y: i32, boundary: CityBoundary) -> i32 {
    if renderer_boundary(boundary) {
        let (start, end) = row_limits(y);
        x.clamp(start, end)
    } else {
        x
    }
}
fn semantic_pixel(map: &CityMap, x: i32, y: i32, boundary: CityBoundary) -> CityPixel {
    let x = if renderer_boundary(boundary) {
        let (start, end) = row_limits(y);
        x.clamp(start, end - 1)
    } else {
        x
    };
    map.pixel(x, y)
}
fn layer_allowed(x: i32, y: i32, boundary: CityBoundary) -> bool {
    if renderer_boundary(boundary) {
        let (start, end) = row_limits(y);
        x > start && x < end
    } else {
        true
    }
}
fn opacity(x: i32, y: i32, boundary: CityBoundary) -> f32 {
    if renderer_boundary(boundary) {
        let (start, end) = row_limits(y);
        1. - ((start - x).max(x - end).max(0) as f32 / 9.).min(1.)
    } else {
        1.
    }
}
fn mesh_tile(map: &CityMap, x: i32, y: i32, color: [f32; 4], boundary: CityBoundary) -> Mesh {
    let vertices = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
        .into_iter()
        .map(|(xx, yy)| {
            let sample = sample_x(xx, yy, boundary);
            let mut color = color;
            color[3] = opacity(xx, yy, boundary);
            Vertex {
                position: Vec3::new(
                    xx as f32,
                    f32::from(map.pixel(sample, yy).elevation) / 12.,
                    yy as f32,
                ),
                normal: map.normal(sample, yy),
                uv: Vec2::new(xx as f32 / 4., yy as f32 / 4.),
                color,
            }
        })
        .collect();
    Mesh {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}
fn kind_order(k: CityPartKind) -> u8 {
    match k {
        CityPartKind::Terrain(c) | CityPartKind::Blend { class: c, .. } => c as u8,
        CityPartKind::RoadEdge(_) => 5,
        CityPartKind::RoadCorner(_) => 6,
    }
}
fn sort_parts(parts: &mut [CityPart]) {
    parts.sort_by_key(|p| (kind_order(p.kind), p.chunk.1, p.chunk.0, p.tile.1, p.tile.0));
}
fn blend_kind(map: &CityMap, x: i32, y: i32, class: TerrainClass) -> Option<(TerrainClass, u8)> {
    let mut absence = 0;
    let mut next = TerrainClass::Water;
    for (dx, dy, bit) in [(0, -1, 2), (1, 0, 1), (0, 1, 8), (-1, 0, 4)] {
        let xx = x + dx;
        let yy = y + dy;
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
    (absence < 15).then_some((next, absence))
}
fn push_part(
    parts: &mut Vec<CityPart>,
    used: &mut (usize, usize),
    cap: usize,
    tile: (u16, u16),
    kind: CityPartKind,
    mesh: Mesh,
) -> Result<(), Error> {
    let vertices = used
        .0
        .checked_add(mesh.vertices.len())
        .ok_or(Error::BudgetExceeded("city vertices"))?;
    let indices = used
        .1
        .checked_add(mesh.indices.len())
        .ok_or(Error::BudgetExceeded("city indices"))?;
    if vertices > cap || vertices > 2_000_000 || indices > 6_000_000 {
        return Err(Error::BudgetExceeded("city geometry"));
    }
    mesh.validate(&RenderLimits::default())
        .map_err(|_| Error::InvalidInput("city part mesh"))?;
    *used = (vertices, indices);
    parts.push(CityPart {
        tile,
        chunk: (tile.0 / 16, tile.1 / 16),
        kind,
        mesh,
    });
    Ok(())
}
fn add_layers(
    parts: &mut Vec<CityPart>,
    used: &mut (usize, usize),
    cap: usize,
    map: &CityMap,
    tile: (u16, u16),
    boundary: CityBoundary,
    mesh: Mesh,
) -> Result<(), Error> {
    let (x, y) = (i32::from(tile.0), i32::from(tile.1));
    let pixel = semantic_pixel(map, x, y, boundary);
    let class = terrain_class(pixel.terrain);
    push_part(
        parts,
        used,
        cap,
        tile,
        CityPartKind::Terrain(class),
        mesh.clone(),
    )?;
    if !layer_allowed(x, y, boundary) {
        return Ok(());
    }
    if let Some((next, mask)) = blend_kind(map, x, y, class) {
        let mut blend = mesh.clone();
        for v in &mut blend.vertices {
            let alpha = v.color[3];
            v.color = white_class(next);
            v.color[3] = alpha;
        }
        // The primary UV remains the terrain coordinate. CityPart::mask_uv()
        // provides the separate, unmirrored 7x3 blend-mask channel.
        push_part(
            parts,
            used,
            cap,
            tile,
            CityPartKind::Blend { class: next, mask },
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
            let offset = Vec2::new(f32::from(index % 8) / 8., f32::from(index / 8) / 4.);
            for v in &mut road.vertices {
                let u = v.position.x - f32::from(tile.0);
                let v_fraction = v.position.z - f32::from(tile.1);
                v.uv = offset + Vec2::new((1. - u) / 8., v_fraction / 4.);
                v.color = [0.25, 0.25, 0.25, 1.];
            }
            push_part(parts, used, cap, tile, kind, road)?;
        }
    }
    Ok(())
}
pub fn build_city_parts(
    map: &CityMap,
    boundary: CityBoundary,
    max_vertices: usize,
) -> Result<Vec<CityPart>, Error> {
    map.validate()?;
    validate_boundary(map, boundary)?;
    let mut parts = Vec::new();
    let mut used = (0usize, 0usize);
    for y in 0..map.height {
        for x in 0..map.width {
            let (xx, yy) = (i32::from(x), i32::from(y));
            if !in_bounds(xx, yy, boundary) {
                continue;
            }
            let class = terrain_class(semantic_pixel(map, xx, yy, boundary).terrain);
            if class == TerrainClass::Void {
                continue;
            }
            let mesh = mesh_tile(map, xx, yy, white_class(class), boundary);
            add_layers(
                &mut parts,
                &mut used,
                max_vertices,
                map,
                (x, y),
                boundary,
                mesh,
            )?;
        }
    }
    sort_parts(&mut parts);
    Ok(parts)
}
fn merge_parts(parts: Vec<CityPart>) -> Result<Mesh, Error> {
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
pub fn build_city_mesh(map: &CityMap, boundary: CityBoundary) -> Result<Mesh, Error> {
    merge_parts(build_city_parts(map, boundary, 2_000_000)?)
}
fn height_cubic_policy(
    map: &CityMap,
    x: i32,
    y: i32,
    u: f32,
    v: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    boundary: CityBoundary,
) -> f32 {
    let mut columns = [0.; 4];
    for (i, c) in columns.iter_mut().enumerate() {
        // Source O() clamps samples to the tile row for dy=-1/0 and to its
        // following row for dy=1/2. Image bounds are then clamped independently.
        let values = [-1, 0, 1, 2].map(|dy| {
            let row = if dy < 1 { y } else { y + 1 };
            let xx = sample_x(x + i as i32 - 1, row, boundary);
            f32::from(map.pixel(xx, y + dy).elevation)
        });
        *c = cubic(values, v, if i < 2 { left } else { right });
    }
    cubic(columns, u, top * (1. - v) + bottom * v) / 12.
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
    height_cubic_policy(
        map,
        x,
        y,
        u,
        v,
        left,
        right,
        top,
        bottom,
        CityBoundary::Rectangle,
    )
}
/// Build all detailed terrain, blend, road-edge, and road-corner layers. Mesh UV
/// is the primary terrain/road coordinate; CityPart::mask_uv supplies one
/// secondary mask coordinate per vertex. The source detailed diagonal is TR-BL.
pub fn build_near_patch_parts(
    map: &CityMap,
    origin: (u16, u16),
    size: (u16, u16),
    subdiv: u8,
    boundary: CityBoundary,
    max_vertices: usize,
) -> Result<Vec<CityPart>, Error> {
    map.validate()?;
    validate_boundary(map, boundary)?;
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
    let mut parts = Vec::new();
    let mut used = (0usize, 0usize);
    for ty in 0..size.1 {
        for tx in 0..size.0 {
            let tile = (origin.0 + tx, origin.1 + ty);
            let (x, y) = (i32::from(tile.0), i32::from(tile.1));
            if !in_bounds(x, y, boundary) {
                continue;
            }
            let class = terrain_class(semantic_pixel(map, x, y, boundary).terrain);
            if class == TerrainClass::Void {
                continue;
            }
            let coarse = mesh_tile(map, x, y, white_class(class), boundary);
            let mut mesh = Mesh {
                vertices: Vec::with_capacity(n * n),
                indices: Vec::with_capacity(usize::from(subdiv) * usize::from(subdiv) * 6),
            };
            let left = if tx == 0 { -1. } else { 0. };
            let right = if tx == size.0 - 1 { -1. } else { 0. };
            let top = if ty == 0 { -1. } else { 0. };
            let bottom = if ty == size.1 - 1 { -1. } else { 0. };
            for iy in 0..n {
                for ix in 0..n {
                    let u = ix as f32 / f32::from(subdiv);
                    let v = iy as f32 / f32::from(subdiv);
                    let h =
                        height_cubic_policy(map, x, y, u, v, left, right, top, bottom, boundary);
                    let a = &coarse.vertices;
                    let normal = a[0]
                        .normal
                        .lerp(a[1].normal, u)
                        .lerp(a[3].normal.lerp(a[2].normal, u), v);
                    let mut color = white_class(class);
                    if !layer_allowed(x, y, boundary) {
                        let top_alpha = a[0].color[3] * (1. - u) + a[1].color[3] * u;
                        let bottom_alpha = a[3].color[3] * (1. - u) + a[2].color[3] * u;
                        color[3] = top_alpha * (1. - v) + bottom_alpha * v;
                    }
                    mesh.vertices.push(Vertex {
                        position: Vec3::new(x as f32 + u, h, y as f32 + v),
                        normal,
                        uv: Vec2::new((x as f32 + u) / 4., (y as f32 + v) / 4.),
                        color,
                    });
                }
            }
            for iy in 0..n - 1 {
                for ix in 0..n - 1 {
                    let a = (iy * n + ix) as u32;
                    mesh.indices.extend([
                        a,
                        a + 1,
                        a + n as u32,
                        a + n as u32,
                        a + 1,
                        a + n as u32 + 1,
                    ]);
                }
            }
            add_layers(
                &mut parts,
                &mut used,
                max_vertices,
                map,
                tile,
                boundary,
                mesh,
            )?;
        }
    }
    sort_parts(&mut parts);
    Ok(parts)
}
/// Convenience geometry merge. Canonical 512x512 maps use the renderer's fade;
/// smaller synthetic maps use rectangular bounds. Use build_near_patch_parts
/// when binding separate materials and blend-mask UV channels.
pub fn build_near_patch(
    map: &CityMap,
    origin: (u16, u16),
    size: (u16, u16),
    subdiv: u8,
) -> Result<Mesh, Error> {
    let boundary = if map.width == 512 && map.height == 512 {
        CityBoundary::RendererWithFade
    } else {
        CityBoundary::Rectangle
    };
    merge_parts(build_near_patch_parts(
        map, origin, size, subdiv, boundary, 2_000_000,
    )?)
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
