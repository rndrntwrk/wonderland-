//! Depth reconstruction and explicit content/parameter/override identity.
use crate::Error;
use sha2::{Digest, Sha256};
use wonderland_render_core::{Aabb, AssetKey, Mesh, RenderLimits, Vec2, Vec3, Vertex};
pub mod resolution;

#[derive(Clone, Debug, PartialEq)]
pub struct ReconstructionParams {
    pub rotations: [bool; 4],
    pub door_fix: bool,
    pub counter_fix: bool,
    pub blender_tweak: bool,
    pub simplify: bool,
    pub start_dgrp: i32,
    pub end_dgrp: i32,
}
impl Default for ReconstructionParams {
    fn default() -> Self {
        Self {
            rotations: [true; 4],
            door_fix: false,
            counter_fix: false,
            blender_tweak: false,
            simplify: true,
            start_dgrp: 0,
            end_dgrp: 0,
        }
    }
}
pub fn select_params(
    dgrp: u16,
    embedded: Option<&ReconstructionParams>,
    filename: Option<&ReconstructionParams>,
) -> ReconstructionParams {
    let selected = embedded.or(filename).cloned().unwrap_or_default();
    if (selected.start_dgrp == 0 && selected.end_dgrp == 0)
        || (i32::from(dgrp) >= selected.start_dgrp && i32::from(dgrp) <= selected.end_dgrp)
    {
        selected
    } else {
        ReconstructionParams::default()
    }
}
pub fn selected_rotations(params: &ReconstructionParams, subindex: u16) -> Vec<u8> {
    if params.door_fix {
        if subindex & 255 == 1 {
            vec![0, 3]
        } else {
            vec![1, 2]
        }
    } else {
        (0..4).filter(|i| params.rotations[*i as usize]).collect()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaPolicy {
    SourceDepthOnly,
    DropTransparent,
}
#[derive(Clone, Copy, Debug)]
pub struct ReconstructionOptions {
    pub alpha: AlphaPolicy,
    pub graphics_units: bool,
    pub max_pixels: usize,
}
impl Default for ReconstructionOptions {
    fn default() -> Self {
        Self {
            alpha: AlphaPolicy::SourceDepthOnly,
            graphics_units: true,
            max_pixels: 16777216,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ReconstructionSprite {
    pub source: AssetKey,
    pub sprite_id: u32,
    pub rotation: u8,
    pub width: u32,
    pub height: u32,
    pub sprite_offset: Vec2,
    pub object_offset: Vec3,
    pub flip: bool,
    pub rgba: Vec<[u8; 4]>,
    pub depth: Option<Vec<u8>>,
    pub dynamic_index: Option<u8>,
}
#[derive(Clone, Debug)]
pub struct ReconstructedPart {
    pub sprite_id: u32,
    pub rotation: u8,
    pub group: u16,
    pub mesh: Mesh,
    pub bounds: Option<Aabb>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ReconstructionWarning {
    SimplifierUnavailable {
        target_triangles: usize,
        iterations: u16,
        aggressiveness: f32,
    },
}
#[derive(Clone, Debug)]
pub struct ReconstructionOutput {
    pub parts: Vec<ReconstructedPart>,
    pub missing_depth: Vec<u32>,
    pub warnings: Vec<ReconstructionWarning>,
    pub completed: usize,
}
fn validate_sprite(s: &ReconstructionSprite, max_pixels: usize) -> Result<usize, Error> {
    if s.width == 0
        || s.height == 0
        || s.width > 4096
        || s.height > 4096
        || s.rotation > 3
        || !s.sprite_offset.is_finite()
        || !s.object_offset.is_finite()
    {
        return Err(Error::InvalidInput("reconstruction sprite metadata"));
    }
    let n = (s.width as usize)
        .checked_mul(s.height as usize)
        .ok_or(Error::BudgetExceeded("sprite pixels"))?;
    if n > max_pixels || n > 16777216 {
        return Err(Error::BudgetExceeded("sprite pixels"));
    }
    if s.rgba.len() != n
        || s.depth.as_ref().map(|d| d.len() != n).unwrap_or(false)
        || s.dynamic_index.map(|i| i >= 128).unwrap_or(false)
    {
        return Err(Error::InvalidInput("reconstruction channels/group"));
    }
    Ok(n)
}
/// Return source mesh tile units; reconstruct optionally converts to graphics units.
pub fn project_pixel(
    sprite: &ReconstructionSprite,
    params: &ReconstructionParams,
    x: f32,
    y: f32,
    depth: u8,
) -> Result<Vec3, Error> {
    validate_sprite(sprite, 16777216)?;
    if !x.is_finite() || !y.is_finite() {
        return Err(Error::InvalidInput("pixel coordinate"));
    }
    let result = project_unchecked(sprite, params, x, y, depth);
    if !result.is_finite() {
        return Err(Error::InvalidInput("projected depth"));
    }
    Ok(result)
}
fn project_unchecked(
    s: &ReconstructionSprite,
    p: &ReconstructionParams,
    x: f32,
    y: f32,
    depth: u8,
) -> Vec3 {
    let x = if s.flip { s.width as f32 - x } else { x };
    let scale = 1.43 / 128.;
    let px = (x + s.sprite_offset.x) * scale;
    let py = -(y + s.sprite_offset.y + 4. - s.height as f32) * scale;
    let factor = if p.blender_tweak { 0.40 } else { 0.39 };
    let zoff = if p.blender_tweak { -57.5 } else { -55. };
    let pz = (zoff + (1. - f32::from(depth) / 255.) * 110.851_25 / factor) * scale;
    let (sx, cx) = (-std::f32::consts::PI / 6.).sin_cos();
    let ry = py * cx - pz * sx;
    let rz = py * sx + pz * cx;
    let (sy, cy) = (std::f32::consts::FRAC_PI_4 * (1. + 2. * f32::from(s.rotation))).sin_cos();
    Vec3::new(
        px * cy + rz * sy + s.object_offset.x / 16.,
        ry + s.object_offset.z / 5.,
        -px * sy + rz * cy + s.object_offset.y / 16.,
    )
}
pub fn triangulate_quad(points: [Option<Vec3>; 4], limit: f32) -> Result<Vec<u8>, Error> {
    if !limit.is_finite() || limit < 0. || points.iter().flatten().any(|p| !p.is_finite()) {
        return Err(Error::InvalidInput("depth discontinuity input"));
    }
    let ids: Vec<_> = points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|_| i as u8))
        .collect();
    let max = limit * limit;
    let ok = |a: usize, b: usize| (points[a].unwrap() - points[b].unwrap()).length_squared() <= max;
    if ids.len() == 4 {
        if ok(0, 2) && ok(1, 3) {
            Ok(vec![0, 1, 2, 0, 2, 3])
        } else {
            Ok(Vec::new())
        }
    } else if ids.len() == 3 {
        if (0..3).all(|i| ok(ids[i] as usize, ids[(i + 1) % 3] as usize)) {
            Ok(ids)
        } else {
            Ok(Vec::new())
        }
    } else {
        Ok(Vec::new())
    }
}
pub fn reconstruct(
    sprites: &[ReconstructionSprite],
    params: &ReconstructionParams,
    subindex: u16,
    options: ReconstructionOptions,
) -> Result<ReconstructionOutput, Error> {
    let mut total = 0usize;
    for s in sprites {
        total = total
            .checked_add(validate_sprite(s, options.max_pixels)?)
            .ok_or(Error::BudgetExceeded("reconstruction pixels"))?;
    }
    if total > options.max_pixels {
        return Err(Error::BudgetExceeded("reconstruction batch pixels"));
    }
    let selected = selected_rotations(params, subindex);
    let mut out = ReconstructionOutput {
        parts: Vec::new(),
        missing_depth: Vec::new(),
        warnings: Vec::new(),
        completed: 0,
    };
    let mut total_vertices = 0;
    let mut total_indices = 0;
    for s in sprites {
        if !selected.contains(&s.rotation) {
            continue;
        }
        out.completed += 1;
        let Some(depth) = &s.depth else {
            out.missing_depth.push(s.sprite_id);
            continue;
        };
        let present = depth
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                **d < 255 && (options.alpha == AlphaPolicy::SourceDepthOnly || s.rgba[*i][3] != 0)
            })
            .count();
        if total_vertices + present > 2000000 {
            return Err(Error::BudgetExceeded("reconstruction vertices"));
        }
        let mut map = vec![None; depth.len()];
        let mut vertices = Vec::with_capacity(present);
        let mut pixels = Vec::with_capacity(present);
        for y in 0..s.height {
            for x in 0..s.width {
                let i = (y * s.width + x) as usize;
                if depth[i] == 255
                    || (options.alpha == AlphaPolicy::DropTransparent && s.rgba[i][3] == 0)
                {
                    continue;
                }
                map[i] = Some(vertices.len() as u32);
                let position = project_unchecked(s, params, x as f32, y as f32, depth[i]);
                pixels.push((x, y));
                vertices.push(Vertex {
                    position,
                    normal: Vec3::ZERO,
                    uv: Vec2::new(x as f32 / s.width as f32, y as f32 / s.height as f32),
                    color: s.rgba[i].map(|v| f32::from(v) / 255.),
                });
            }
        }
        let mut indices = Vec::new();
        for y in 0..s.height - 1 {
            for x in 0..s.width - 1 {
                let i = (y * s.width + x) as usize;
                let q = [
                    map[i],
                    map[i + 1],
                    map[i + 1 + s.width as usize],
                    map[i + s.width as usize],
                ];
                let points = q.map(|id| id.map(|i| vertices[i as usize].position));
                for id in triangulate_quad(points, 0.065)? {
                    indices.push(q[id as usize].unwrap());
                }
                if indices.len() + total_indices > 6000000 {
                    return Err(Error::BudgetExceeded("reconstruction indices"));
                }
            }
        }
        if params.counter_fix {
            let mut positions: Vec<_> = vertices.iter().map(|v| v.position).collect();
            counter_correct(&mut positions, &pixels, s.rotation)?;
            for (v, p) in vertices.iter_mut().zip(positions) {
                v.position = p;
            }
        }
        if params.simplify {
            out.warnings
                .push(ReconstructionWarning::SimplifierUnavailable {
                    target_triangles: indices.len() / 3 / 100,
                    iterations: 125,
                    aggressiveness: 3.5,
                });
        }
        if indices.is_empty() {
            continue;
        }
        for tri in indices.chunks_exact(3) {
            let a = vertices[tri[0] as usize].position;
            let b = vertices[tri[1] as usize].position;
            let c = vertices[tri[2] as usize].position;
            let mut n = (b - a).cross(c - a);
            if !s.flip {
                n = -n;
            }
            for id in tri {
                let v = &mut vertices[*id as usize];
                v.normal = v.normal + n;
            }
        }
        // Orphan vertices cannot affect contact bounds or retained memory.
        let mut used = vec![false; vertices.len()];
        for i in &indices {
            used[*i as usize] = true;
        }
        let mut remap = vec![0; vertices.len()];
        let mut compact = Vec::new();
        for (i, mut v) in vertices.into_iter().enumerate() {
            if used[i] {
                v.normal = v.normal.normalize_or_zero();
                if options.graphics_units {
                    v.position = v.position * 3.;
                }
                remap[i] = compact.len() as u32;
                compact.push(v);
            }
        }
        for i in &mut indices {
            *i = remap[*i as usize];
        }
        let mesh = Mesh {
            vertices: compact,
            indices,
        };
        mesh.validate(&RenderLimits::default())
            .map_err(|_| Error::InvalidInput("reconstructed mesh"))?;
        let bounds =
            Aabb::from_points(&mesh.vertices.iter().map(|v| v.position).collect::<Vec<_>>());
        total_vertices += mesh.vertices.len();
        total_indices += mesh.indices.len();
        out.parts.push(ReconstructedPart {
            sprite_id: s.sprite_id,
            rotation: s.rotation,
            group: s.dynamic_index.map(|i| u16::from(i) + 1).unwrap_or(0),
            mesh,
            bounds,
        });
    }
    Ok(out)
}
pub fn derivation_key(
    sprites: &[ReconstructionSprite],
    params: &ReconstructionParams,
    subindex: u16,
    options: ReconstructionOptions,
    base: AssetKey,
    patch: AssetKey,
) -> Result<AssetKey, Error> {
    let mut hash = Sha256::new();
    hash.update(b"wonderland-depth-reconstruction-v1\0");
    hash.update(base.0);
    hash.update(patch.0);
    hash.update(subindex.to_le_bytes());
    hash.update(params.rotations.map(u8::from));
    hash.update([
        u8::from(params.door_fix),
        u8::from(params.counter_fix),
        u8::from(params.blender_tweak),
        u8::from(params.simplify),
    ]);
    hash.update(params.start_dgrp.to_le_bytes());
    hash.update(params.end_dgrp.to_le_bytes());
    hash.update([options.alpha as u8, u8::from(options.graphics_units)]);
    hash.update((sprites.len() as u64).to_le_bytes());
    let mut total = 0usize;
    for s in sprites {
        total = total
            .checked_add(validate_sprite(s, options.max_pixels)?)
            .ok_or(Error::BudgetExceeded("hash pixels"))?;
        if total > options.max_pixels {
            return Err(Error::BudgetExceeded("hash pixels"));
        }
        hash.update(s.source.0);
        hash.update(s.sprite_id.to_le_bytes());
        hash.update([s.rotation, u8::from(s.flip)]);
        hash.update(s.width.to_le_bytes());
        hash.update(s.height.to_le_bytes());
        for v in [
            s.sprite_offset.x,
            s.sprite_offset.y,
            s.object_offset.x,
            s.object_offset.y,
            s.object_offset.z,
        ] {
            hash.update(v.to_bits().to_le_bytes());
        }
        hash.update(
            s.dynamic_index
                .map(|i| u16::from(i) + 1)
                .unwrap_or(0)
                .to_le_bytes(),
        );
        for p in &s.rgba {
            hash.update(p);
        }
        if let Some(d) = &s.depth {
            hash.update([1]);
            hash.update(d);
        } else {
            hash.update([0]);
        }
    }
    Ok(AssetKey(hash.finalize().into()))
}
pub fn counter_correct(
    positions: &mut [Vec3],
    pixels: &[(u32, u32)],
    rotation: u8,
) -> Result<(), Error> {
    if positions.len() != pixels.len() || rotation > 3 || positions.iter().any(|p| !p.is_finite()) {
        return Err(Error::InvalidInput("counter correction"));
    }
    let outside = positions.iter().filter(|p| p.x.abs() > 0.4).count();
    if outside
        .checked_mul(positions.len())
        .map(|work| work > 20_000_000)
        .unwrap_or(true)
    {
        return Err(Error::BudgetExceeded("counter nearest-border comparisons"));
    }
    let original = positions.to_vec();
    let edge = 0.498 + 0.001 * f32::from(rotation % 2);
    for (i, p) in positions.iter_mut().enumerate() {
        if p.x.abs() <= 0.4 {
            continue;
        }
        let sign = if p.x > 0. { 1. } else { -1. };
        let mut best = None;
        let mut distance = f64::INFINITY;
        for (j, border) in original.iter().enumerate() {
            if border.x * sign <= 0.38 || border.x * sign > 0.4 {
                continue;
            }
            let dx = f64::from(pixels[i].0) - f64::from(pixels[j].0);
            let dy = f64::from(pixels[i].1) - f64::from(pixels[j].1);
            let d = dx * dx + dy * dy;
            if d < distance {
                distance = d;
                best = Some(*border);
            }
        }
        if let Some(border) = best {
            p.x = border.x + sign * (distance.sqrt() as f32) / 71.55;
            if p.x.abs() > 0.5 {
                p.x = sign * edge;
            } else {
                p.y = border.y;
                p.z = border.z;
            }
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskType {
    None,
    Normal,
    Portal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthPass {
    ClearDepth0,
    ClearDepth1,
    Body(u16),
    PortalFinal(u16),
    ClearPortalStencil,
}
pub fn depth_passes(
    mask: MaskType,
    groups: u16,
    visible: [u64; 2],
    lightmap: bool,
) -> Result<Vec<DepthPass>, Error> {
    if groups > 129 {
        return Err(Error::InvalidInput("legacy dynamic group limit"));
    }
    if groups == 0 || (lightmap && mask == MaskType::Portal) {
        return Ok(Vec::new());
    }
    let mut passes = Vec::new();
    if !lightmap && mask != MaskType::None {
        passes.extend([DepthPass::ClearDepth0, DepthPass::ClearDepth1]);
    }
    for group in 0..groups {
        if mask == MaskType::Portal && group == groups - 1 {
            continue;
        }
        if group == 0 || visible[usize::from((group - 1) / 64)] & (1u64 << ((group - 1) % 64)) != 0
        {
            passes.push(DepthPass::Body(group));
        }
    }
    if mask == MaskType::Portal {
        passes.extend([
            DepthPass::PortalFinal(groups - 1),
            DepthPass::ClearPortalStencil,
        ]);
    }
    Ok(passes)
}
pub fn contact_translation(bounds: Aabb, anchor: Vec3) -> Result<Vec3, Error> {
    if !anchor.is_finite() || Aabb::new(bounds.min, bounds.max).is_none() {
        return Err(Error::InvalidInput("contact bounds/anchor"));
    }
    Ok(Vec3::new(
        anchor.x - (bounds.min.x + bounds.max.x) / 2.,
        anchor.y - bounds.min.y,
        anchor.z - (bounds.min.z + bounds.max.z) / 2.,
    ))
}
