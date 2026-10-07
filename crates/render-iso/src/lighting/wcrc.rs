//! Executable Blueprint.WCRC sunlight capture and filtering.
//! Source: LMapBatch.DrawWallShadows/CreateOutsideIfMissing, WallComponentRC.DrawLMap,
//! TerrainComponent/3DFloorGeometry.DrawLMap, RoofComponent.DrawLMap,
//! RCObject.fx WallLMap and SpriteEffects.fx OutdoorsPCFStage1..4.
//! The source component's presence is explicit, independent of view mode/Ultra.
use super::*;
use wonderland_render_core::{cache::DerivedKey, AssetKey, RgbaImage, Vec2, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct WcrcShadowTexture {
    pub source: AssetKey,
    pub image: RgbaImage,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WcrcWallVertex {
    /// Raw source wall coordinates: tile X/Y and wall height Z. Cutaways are
    /// restored from texture.y, just as vsWallLMap restores position.z.
    pub position: Vec3,
    pub texture: Vec2,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcrcWallGroup {
    pub source: AssetKey,
    pub floor: u8,
    pub use_offset: bool,
    pub mask: WcrcShadowTexture,
    pub vertices: Vec<WcrcWallVertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcrcHorizontalMesh {
    pub source: AssetKey,
    pub floor: u8,
    /// Effective source floor/roof triangles in graphics units (three per tile).
    /// Source DrawLMap flattens Y and substitutes the group level.
    pub vertices: Vec<Vec3>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcrcObjectMeshes {
    /// The source's visible outdoor object draw list, already transformed
    /// relative to this target floor, including higher-floor container offsets.
    pub target_floor: u8,
    pub meshes: Vec<MeshShadowInput>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcrcGeometryInput {
    pub source: AssetKey,
    /// Divider of the source's currently allocated target. It changes on the
    /// following allocation, so it is explicit rather than inferred from time.
    pub quality_divider: u8,
    /// Preserve source floor/group iteration order: blend-channel transitions
    /// depend on UseOffset, including the initial false group starting in red.
    pub walls: Vec<WcrcWallGroup>,
    pub floors: Vec<WcrcHorizontalMesh>,
    pub roofs: Vec<WcrcHorizontalMesh>,
    pub objects: Vec<WcrcObjectMeshes>,
    /// Snapshot of TextureGenerator.GetUniformNoise, never the VM RNG. The
    /// source normally supplies a 512x512 image; smaller explicit fixtures work.
    pub noise: WcrcShadowTexture,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedWcrcCapture {
    key: DerivedKey,
    floor: u8,
    image: RgbaImage,
    raster_samples: u64,
}
impl PreparedWcrcCapture {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn floor(&self) -> u8 {
        self.floor
    }
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }
    pub fn raster_samples(&self) -> u64 {
        self.raster_samples
    }
    pub fn resident_bytes(&self) -> usize {
        self.image.pixels.capacity() * 4
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedWcrcSunlight {
    key: DerivedKey,
    pub(super) room_map_key: DerivedKey,
    pub(super) sunlight: DirectionalLighting,
    pub(super) ultra: bool,
    pub(super) software_depth: bool,
    floors: Vec<PreparedWcrcCapture>,
    resident_bytes: usize,
    raster_samples: u64,
}
impl PreparedWcrcSunlight {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn floors(&self) -> &[PreparedWcrcCapture] {
        &self.floors
    }
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }
    pub fn raster_samples(&self) -> u64 {
        self.raster_samples
    }
    pub(super) fn floor(&self, floor: u8) -> Option<&RgbaImage> {
        self.floors
            .iter()
            .find(|f| f.floor == floor)
            .map(|f| &f.image)
    }
}
/// Preserves the two independent source `if`s: >6 sets four and then >3
/// overwrites it with two. Four is never the resulting source setting.
pub fn source_outside_quality_divider(falloff: f32) -> Result<u8> {
    if !falloff.is_finite() || !(0. ..=10_000.).contains(&falloff) {
        return Err(IsoError::Invalid("outside shadow falloff"));
    }
    Ok(if falloff > 3. { 2 } else { 1 })
}
fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or(IsoError::Limit("WCRC bytes"))
}
fn charge(work: &mut u64, amount: u64, budget: LightingBudget) -> Result<()> {
    *work = work
        .checked_add(amount)
        .ok_or(IsoError::Limit("WCRC work"))?;
    if *work > budget.max_raster_samples {
        return Err(IsoError::Limit("WCRC raster work"));
    }
    Ok(())
}
fn texture_valid(t: &WcrcShadowTexture, b: LightingBudget) -> Result<usize> {
    let count = b.texture(t.image.width, t.image.height)?;
    if count != t.image.pixels.len() {
        return Err(IsoError::Invalid("WCRC texture pixel count"));
    }
    Ok(count * 4 + 48)
}
fn mesh_valid(vertices: &[Vec3], indices: &[u32], budget: LightingBudget) -> Result<()> {
    if vertices.len() > budget.max_vertices || indices.len() > budget.max_indices {
        return Err(IsoError::Limit("WCRC geometry"));
    }
    if indices.len() % 3 != 0
        || indices.iter().any(|&i| i as usize >= vertices.len())
        || vertices.iter().any(|v| {
            !v.is_finite()
                || v.x.abs() > 1_000_000.
                || v.y.abs() > 1_000_000.
                || v.z.abs() > 1_000_000.
        })
    {
        return Err(IsoError::Invalid("WCRC triangle input"));
    }
    Ok(())
}
fn validate(
    input: &WcrcGeometryInput,
    maps: &PreparedRoomMaps,
    sun: DirectionalLighting,
    ultra: bool,
    software_depth: bool,
    b: LightingBudget,
) -> Result<([u32; 2], usize, u64)> {
    sun.light.validate()?;
    if sun.light.kind != ShadowLightKind::Directional
        || !sun.shadow_multiplier.is_finite()
        || !(0. ..=1.33).contains(&sun.shadow_multiplier)
        || !sun.sun_direction.is_finite()
        || (sun.sun_direction.length() - 1.).abs() > 0.0001
        || !(1..=2).contains(&input.quality_divider)
    {
        return Err(IsoError::Invalid("WCRC sunlight/divider"));
    }
    if input.noise.image.width != input.noise.image.height
        || !input.noise.image.width.is_power_of_two()
    {
        return Err(IsoError::Invalid("WCRC source noise dimensions"));
    }
    let layout = LightAtlasLayout::new(
        maps.width(),
        maps.height(),
        ultra,
        software_depth,
        b.max_texture_pixels,
    )?;
    let size = layout
        .wall_shadow
        .map(|v| v * 2 / u32::from(input.quality_divider));
    b.texture(size[0], size[1])?;
    let mut count =
        input.walls.len() + input.floors.len() + input.roofs.len() + input.objects.len();
    if count > b.max_occluders {
        return Err(IsoError::Limit("WCRC group count"));
    }
    let (mut vertices, mut indices, mut bytes) =
        (0usize, 0usize, add(256, texture_valid(&input.noise, b)?)?);
    if input.walls.windows(2).any(|g| g[0].floor > g[1].floor) {
        return Err(IsoError::Invalid("WCRC wall source floor order"));
    }
    for g in &input.walls {
        if g.floor as usize >= maps.maps().len()
            || g.indices.len() % 3 != 0
            || g.indices.iter().any(|&i| i as usize >= g.vertices.len())
            || g.vertices.iter().any(|v| {
                !v.position.is_finite()
                    || !v.texture.is_finite()
                    || [
                        v.position.x,
                        v.position.y,
                        v.position.z,
                        v.texture.x,
                        v.texture.y,
                    ]
                    .iter()
                    .any(|v| v.abs() > 1_000_000.)
            })
        {
            return Err(IsoError::Invalid("WCRC wall input"));
        }
        vertices = add(vertices, g.vertices.len())?;
        indices = add(indices, g.indices.len())?;
        bytes = add(
            bytes,
            add(
                128 + g.vertices.len() * 20 + g.indices.len() * 4,
                texture_valid(&g.mask, b)?,
            )?,
        )?;
    }
    for g in input.floors.iter().chain(&input.roofs) {
        if g.floor as usize >= maps.maps().len() {
            return Err(IsoError::Invalid("WCRC horizontal floor"));
        }
        mesh_valid(&g.vertices, &g.indices, b)?;
        vertices = add(vertices, g.vertices.len())?;
        indices = add(indices, g.indices.len())?;
        bytes = add(bytes, 128 + g.vertices.len() * 12 + g.indices.len() * 4)?;
    }
    let mut seen = [false; 5];
    for group in &input.objects {
        if group.target_floor as usize >= maps.maps().len() || seen[group.target_floor as usize] {
            return Err(IsoError::Invalid("WCRC object target floor"));
        }
        seen[group.target_floor as usize] = true;
        count = add(count, group.meshes.len())?;
        bytes = add(bytes, 16)?;
        for g in &group.meshes {
            super::projected::validate_mesh(g, b)?;
            vertices = add(vertices, g.vertices.len())?;
            indices = add(indices, g.indices.len())?;
            bytes = add(bytes, 128 + g.vertices.len() * 12 + g.indices.len() * 4)?;
        }
    }
    if count > b.max_occluders || vertices > b.max_vertices || indices > b.max_indices {
        return Err(IsoError::Limit("WCRC cumulative geometry"));
    }
    Ok((size, bytes, (vertices + indices + count) as u64))
}
fn encode_texture(bytes: &mut Vec<u8>, t: &WcrcShadowTexture) {
    bytes.extend_from_slice(&t.source.0);
    bytes.extend_from_slice(&t.image.width.to_le_bytes());
    bytes.extend_from_slice(&t.image.height.to_le_bytes());
    for p in &t.image.pixels {
        bytes.extend_from_slice(p);
    }
}
fn encode_vec3(bytes: &mut Vec<u8>, v: Vec3) {
    for x in [v.x, v.y, v.z] {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
}
fn encode_mesh(
    bytes: &mut Vec<u8>,
    source: AssetKey,
    floor: u8,
    vertices: &[Vec3],
    indices: &[u32],
) {
    bytes.extend_from_slice(&source.0);
    bytes.push(floor);
    bytes.extend_from_slice(&(vertices.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(indices.len() as u64).to_le_bytes());
    for &v in vertices {
        encode_vec3(bytes, v);
    }
    for i in indices {
        bytes.extend_from_slice(&i.to_le_bytes());
    }
}
fn capture_key(
    input: &WcrcGeometryInput,
    maps: &PreparedRoomMaps,
    sun: DirectionalLighting,
    ultra: bool,
    software_depth: bool,
    floor: u8,
    capacity: usize,
) -> DerivedKey {
    let mut b = Vec::with_capacity(capacity);
    b.extend_from_slice(b"source-WCRC-capture-v1\0");
    b.extend_from_slice(&maps.key().0);
    b.extend_from_slice(&[
        floor,
        input.quality_divider,
        ultra as u8,
        software_depth as u8,
    ]);
    for f in [
        sun.light.direction.x,
        sun.light.direction.y,
        sun.light.falloff_multiplier,
        sun.shadow_multiplier,
        sun.sun_direction.x,
        sun.sun_direction.y,
        sun.sun_direction.z,
    ] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    encode_texture(&mut b, &input.noise);
    b.extend_from_slice(&(input.walls.len() as u64).to_le_bytes());
    for g in &input.walls {
        b.extend_from_slice(&g.source.0);
        b.extend_from_slice(&[g.floor, g.use_offset as u8]);
        encode_texture(&mut b, &g.mask);
        b.extend_from_slice(&(g.vertices.len() as u64).to_le_bytes());
        b.extend_from_slice(&(g.indices.len() as u64).to_le_bytes());
        for v in &g.vertices {
            encode_vec3(&mut b, v.position);
            b.extend_from_slice(&v.texture.x.to_le_bytes());
            b.extend_from_slice(&v.texture.y.to_le_bytes());
        }
        for i in &g.indices {
            b.extend_from_slice(&i.to_le_bytes());
        }
    }
    for groups in [&input.floors, &input.roofs] {
        b.extend_from_slice(&(groups.len() as u64).to_le_bytes());
        for g in groups {
            encode_mesh(&mut b, g.source, g.floor, &g.vertices, &g.indices);
        }
    }
    b.extend_from_slice(&(input.objects.len() as u64).to_le_bytes());
    for group in &input.objects {
        b.push(group.target_floor);
        b.extend_from_slice(&(group.meshes.len() as u64).to_le_bytes());
        for g in &group.meshes {
            encode_mesh(
                &mut b,
                g.source,
                group.target_floor,
                &g.vertices,
                &g.indices,
            );
        }
    }
    debug_assert!(b.len() <= capacity);
    debug_assert_eq!(b.capacity(), capacity);
    DerivedKey::new(input.source, maps.source(), 1, &b)
}
#[derive(Clone, Copy)]
struct CaptureVertex {
    position: Vec2,
    uv: Vec2,
    strength: f32,
}
fn project(x: f32, y: f32, height: f32, light: ShadowLight) -> Vec2 {
    Vec2::new(x * 16., y * 16.) + light.direction * (height * 32. * light.falloff_multiplier)
}
fn frac(v: f32) -> f32 {
    v - v.floor()
}
fn edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
fn point(image: &RgbaImage, uv: Vec2, wrap: bool) -> [f32; 4] {
    let sample = |v: f32, n: u32| {
        if wrap {
            (frac(v) * n as f32).floor() as u32
        } else {
            (v * n as f32).floor().clamp(0., n as f32 - 1.) as u32
        }
    };
    image.pixels[(sample(uv.y, image.height) * image.width + sample(uv.x, image.width)) as usize]
        .map(|v| f32::from(v) / 255.)
}
fn mask_alpha(image: &RgbaImage, uv: Vec2) -> f32 {
    let p = Vec2::new(
        uv.x * image.width as f32 - 0.5,
        uv.y * image.height as f32 - 0.5,
    );
    let x = p.x.floor();
    let y = p.y.floor();
    let (fx, fy) = (p.x - x, p.y - y);
    let at = |dx: f32, dy: f32| {
        f32::from(
            image.pixels[((y + dy).clamp(0., image.height as f32 - 1.) as u32 * image.width
                + (x + dx).clamp(0., image.width as f32 - 1.) as u32)
                as usize][3],
        ) / 255.
    };
    (at(0., 0.) * (1. - fx) + at(1., 0.) * fx) * (1. - fy)
        + (at(0., 1.) * (1. - fx) + at(1., 1.) * fx) * fy
}
fn raster(
    image: &mut RgbaImage,
    extent: Vec2,
    mut v: [CaptureVertex; 3],
    channel: usize,
    mask: Option<(&RgbaImage, bool, u8)>,
    work: &mut u64,
    b: LightingBudget,
) -> Result<()> {
    for p in &mut v {
        p.position = Vec2::new(
            p.position.x / extent.x * image.width as f32,
            p.position.y / extent.y * image.height as f32,
        );
    }
    let mut area = edge(v[0].position, v[1].position, v[2].position);
    if area.abs() < 1e-12 {
        return Ok(());
    }
    if area < 0. {
        v.swap(1, 2);
        area = -area;
    }
    let minx = v
        .iter()
        .map(|v| v.position.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., image.width as f32) as u32;
    let maxx = v
        .iter()
        .map(|v| v.position.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., image.width as f32) as u32;
    let miny = v
        .iter()
        .map(|v| v.position.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., image.height as f32) as u32;
    let maxy = v
        .iter()
        .map(|v| v.position.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., image.height as f32) as u32;
    charge(
        work,
        u64::from(maxx - minx) * u64::from(maxy - miny) * if mask.is_some() { 5 } else { 1 },
        b,
    )?;
    for y in miny..maxy {
        for x in minx..maxx {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w = [
                edge(v[1].position, v[2].position, p),
                edge(v[2].position, v[0].position, p),
                edge(v[0].position, v[1].position, p),
            ];
            if w.iter().any(|&w| w < -1e-5) {
                continue;
            }
            let w = w.map(|w| w / area);
            let strength = v[0].strength * w[0] + v[1].strength * w[1] + v[2].strength * w[2];
            if let Some((mask, offset, floor)) = mask {
                let uv = v[0].uv * w[0] + v[1].uv * w[1] + v[2].uv * w[2];
                if uv.y - 0.001 < f32::from(floor) {
                    continue;
                }
                let x = frac(uv.x);
                let y = if offset {
                    frac(frac(uv.y) * 0.970 * (-(1. - 0.1185)) + (1. - x) * 0.1185 - 0.117)
                } else {
                    frac(((uv.y % 1.) - 1. / 240.) / -1.04)
                };
                if strength * mask_alpha(mask, Vec2::new(x, y)) < 0.02 {
                    continue;
                }
            }
            let out = &mut image.pixels[(y * image.width + x) as usize];
            out[channel] = out[channel].max((strength * 255.).round().clamp(0., 255.) as u8);
        }
    }
    Ok(())
}
/// Produces the actual unfiltered red/green RGBA8 target for one floor. This is
/// also useful for uploading the capture to an equivalent GPU PCF implementation.
pub fn capture_wcrc_outside(
    input: &WcrcGeometryInput,
    maps: &PreparedRoomMaps,
    sun: DirectionalLighting,
    ultra: bool,
    software_depth: bool,
    floor: u8,
    budget: LightingBudget,
) -> Result<PreparedWcrcCapture> {
    let (size, key_bytes, mut work) = validate(input, maps, sun, ultra, software_depth, budget)?;
    if floor as usize >= maps.maps().len() {
        return Err(IsoError::Invalid("WCRC capture floor"));
    }
    let pixels = budget.texture(size[0], size[1])?;
    budget.bytes(add(add(maps.resident_bytes(), pixels * 4)?, key_bytes)?)?;
    charge(&mut work, pixels as u64, budget)?;
    let key = capture_key(input, maps, sun, ultra, software_depth, floor, key_bytes);
    let mut image = RgbaImage {
        width: size[0],
        height: size[1],
        pixels: vec![[0, 0, 0, 255]; pixels],
    };
    let extent = Vec2::new(
        (maps.width() - 1) as f32 * 16.,
        (maps.height() - 1) as f32 * 16.,
    );
    let (mut last_side, mut channel) = (false, 0);
    for g in input
        .walls
        .iter()
        .filter(|g| g.floor >= floor && !g.indices.is_empty())
    {
        if last_side != g.use_offset {
            channel = if g.use_offset { 0 } else { 1 };
            last_side = g.use_offset;
        }
        for tri in g.indices.chunks_exact(3) {
            let v = std::array::from_fn(|i| {
                let v = g.vertices[tri[i] as usize];
                let y = (v.texture.y - 0.001).ceil();
                // Multiplying the source 0.07/falloff translation analytically avoids
                // a 0/0 at overhead sun, while retaining its finite 2.24-unit limit.
                CaptureVertex {
                    position: project(v.position.x, v.position.y, y - f32::from(floor), sun.light)
                        + sun.light.direction * 2.24,
                    uv: Vec2::new(v.texture.x, y),
                    strength: 1. - (y - f32::from(floor)) / 5.,
                }
            });
            raster(
                &mut image,
                extent,
                v,
                channel,
                Some((&g.mask.image, g.use_offset, floor)),
                &mut work,
                budget,
            )?;
        }
    }
    for (roof, groups) in [(false, &input.floors), (true, &input.roofs)] {
        for g in groups.iter().filter(|g| {
            if roof {
                g.floor >= floor
            } else {
                g.floor > floor
            }
        }) {
            let height = f32::from(g.floor) - f32::from(floor) + if roof { 1. } else { 0. };
            for tri in g.indices.chunks_exact(3) {
                let v = std::array::from_fn(|i| {
                    let p = g.vertices[tri[i] as usize];
                    CaptureVertex {
                        position: project(p.x / 3., p.z / 3., height, sun.light),
                        uv: Vec2::ZERO,
                        strength: 1. - height / 5.,
                    }
                });
                raster(&mut image, extent, v, 0, None, &mut work, budget)?;
            }
        }
    }
    if ultra {
        if let Some(group) = input.objects.iter().find(|g| g.target_floor == floor) {
            for g in &group.meshes {
                for tri in g.indices.chunks_exact(3) {
                    let v = std::array::from_fn(|i| {
                        let p = g.vertices[tri[i] as usize];
                        CaptureVertex {
                            position: project(p.x / 3., p.z / 3., p.y / 9., sun.light),
                            uv: Vec2::ZERO,
                            strength: 1. - p.y / (3. * 2.95 * 5.),
                        }
                    });
                    raster(&mut image, extent, v, 1, None, &mut work, budget)?;
                }
            }
        }
    }
    Ok(PreparedWcrcCapture {
        key,
        floor,
        image,
        raster_samples: work,
    })
}
const GAUSSIAN: [f32; 23] = [
    0.000169, 0.000538, 0.001532, 0.003907, 0.008922, 0.018249, 0.033435, 0.054872, 0.080666,
    0.106223, 0.125294, 0.132384, 0.125294, 0.106223, 0.080666, 0.054872, 0.033435, 0.018249,
    0.008922, 0.003907, 0.001532, 0.000538, 0.000169,
];
fn quantize(v: [f32; 4]) -> [u8; 4] {
    v.map(|v| (v * 255.).round().clamp(0., 255.) as u8)
}
#[derive(Clone, Copy)]
struct PcfUniforms {
    amount: [f32; 2],
    height: [f32; 2],
    harden: [f32; 2],
}
fn pcf_strength(info: [f32; 4], u: PcfUniforms) -> [f32; 2] {
    std::array::from_fn(|c| {
        let numerator = ((1. - info[c + 2]) * u.height[c] - u.harden[c]).max(0.);
        let denominator = 1. - u.harden[c];
        // The original is undefined at exactly harden=1 and numerator=0.
        // Preserve the finite zero limit at that isolated presentation boundary.
        (if denominator == 0. {
            if numerator == 0. {
                0.
            } else {
                1.
            }
        } else {
            (numerator / denominator).min(1.)
        }) * u.amount[c]
    })
}
fn pcf_pixel(
    stage: usize,
    src: &RgbaImage,
    noise: &RgbaImage,
    uv: Vec2,
    info: [f32; 4],
    constant: bool,
    u: PcfUniforms,
) -> [u8; 4] {
    let sample = |uv| {
        if constant {
            info
        } else {
            point(src, uv, false)
        }
    };
    let mut out = info;
    if stage < 2 {
        let (mut number, mut sum) = ([0f32; 2], [0f32; 2]);
        for (i, &gaussian) in GAUSSIAN.iter().enumerate() {
            for c in 0..2 {
                let delta = (i as f32 - 11.) * u.amount[c];
                let p = sample(
                    uv + if stage == 0 {
                        Vec2::new(delta, 0.)
                    } else {
                        Vec2::new(0., delta)
                    },
                );
                let weight = if stage == 0 {
                    if p[c] > 0. {
                        gaussian / 0.132384
                    } else {
                        0.
                    }
                } else {
                    p[c + 2] * 23.
                };
                number[c] += weight;
                sum[c] += p[c] * weight;
            }
        }
        for c in 0..2 {
            number[c] = number[c].max(0.0001);
            if stage == 0 {
                out[c] = sum[c] / number[c];
                out[c + 2] = number[c] / 23.;
            } else {
                out[c + 2] = sum[c] / number[c];
            }
        }
    } else if info[2] == 0. && info[3] == 0. {
        out = [0., 0., info[2], info[3]];
    } else if info[2] == 1. && info[3] == 1. {
        out = [1., 1., info[2], info[3]];
    } else {
        let noise = point(noise, uv * 8., true)[stage - 2] - 0.5;
        out = [
            0.,
            0.,
            if stage == 2 { info[2] } else { 0. },
            if stage == 2 { info[3] } else { 1. },
        ];
        let strength = pcf_strength(info, u);
        for c in 0..2 {
            let offset = noise * strength[c];
            let noised = uv
                + if stage == 2 {
                    Vec2::new(offset, 0.)
                } else {
                    Vec2::new(0., offset)
                };
            for (i, &weight) in GAUSSIAN.iter().enumerate() {
                let d = (i as f32 - 11.) * strength[c];
                let p = sample(
                    noised
                        + if stage == 2 {
                            Vec2::new(d, 0.)
                        } else {
                            Vec2::new(0., d)
                        },
                )[c];
                out[c] += weight * if stage == 2 { p.ceil() } else { p };
            }
        }
    }
    quantize(out)
}
// TextureGenerator.GetUniformNoise uses UploadWithMips -> Decimate, which
// averages RGB of nontransparent texels and takes MAX alpha at every level.
// NoiseSampler's implicit derivatives see UV*8; its mip filter is POINT.
fn source_noise_mip(
    noise: &RgbaImage,
    target: [u32; 2],
    work: &mut u64,
    budget: LightingBudget,
) -> Result<Option<RgbaImage>> {
    let rho = (noise.width as f32 * 8. / target[0] as f32)
        .max(noise.height as f32 * 8. / target[1] as f32);
    let level = (rho.log2().round().max(0.) as u32).min(noise.width.trailing_zeros());
    let mut current: Option<RgbaImage> = None;
    for _ in 0..level {
        let old = current.as_ref().unwrap_or(noise);
        let width = (old.width / 2).max(1);
        let height = (old.height / 2).max(1);
        let count = width as usize * height as usize;
        budget.bytes(count * 4 + current.as_ref().map_or(0, |m| m.pixels.capacity() * 4))?;
        charge(work, count as u64 * 5, budget)?;
        let mut pixels = Vec::with_capacity(count);
        for y in 0..height {
            for x in 0..width {
                let (mut sum, mut alpha, mut n) = ([0u32; 3], 0u8, 0u32);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let p = old.pixels[((y * 2 + dy) * old.width + x * 2 + dx) as usize];
                        alpha = alpha.max(p[3]);
                        if p[3] > 0 {
                            for c in 0..3 {
                                sum[c] += u32::from(p[c]);
                            }
                            n += 1;
                        }
                    }
                }
                let n = n.max(1);
                pixels.push([
                    (sum[0] / n) as u8,
                    (sum[1] / n) as u8,
                    (sum[2] / n) as u8,
                    alpha,
                ]);
            }
        }
        current = Some(RgbaImage {
            width,
            height,
            pixels,
        });
    }
    Ok(current)
}
fn blur(
    image: &mut RgbaImage,
    noise: &RgbaImage,
    width: u32,
    falloff: f32,
    work: &mut u64,
    budget: LightingBudget,
) -> Result<()> {
    let pixels = image.pixels.len();
    let line = image.width.max(image.height) as usize;
    // Only one reusable run table is live; no per-pixel full-image index table.
    budget.bytes(add(pixels * 8, line * 4)?)?;
    let amount = (0.2 / width as f32) * (f64::from(falloff).powf(0.8) as f32);
    let height = (falloff / 1.5).max(1.);
    let harden = 0.03 * falloff.sqrt();
    let u = PcfUniforms {
        amount: [amount, amount * 2. / 5.],
        height: [height, height * 5. / 2.],
        harden: [harden, harden * 0.5],
    };
    charge(work, pixels as u64, budget)?;
    let first = image.pixels[0];
    if image.pixels.iter().all(|&p| p == first) {
        // The exact constant-capture result still executes the four quantized
        // shaders and BA-only stage. Every sampled coordinate has the same value.
        charge(work, pixels as u64 + 4 * 49, budget)?;
        let mut p = first;
        for stage in 0..4 {
            let out = pcf_pixel(
                stage,
                image,
                noise,
                Vec2::ZERO,
                p.map(|v| f32::from(v) / 255.),
                true,
                u,
            );
            p = if stage == 1 {
                [first[0], first[1], out[2], out[3]]
            } else {
                out
            };
        }
        image.pixels.fill(p);
        return Ok(());
    }
    let noise_budget = LightingBudget {
        max_total_bytes: budget
            .max_total_bytes
            .checked_sub(pixels * 8 + line * 4)
            .ok_or(IsoError::Limit("WCRC noise mip bytes"))?,
        ..budget
    };
    let noise_mip = source_noise_mip(noise, [image.width, image.height], work, noise_budget)?;
    let noise = noise_mip.as_ref().unwrap_or(noise);
    charge(work, pixels as u64, budget)?;
    let mut post = RgbaImage {
        width: image.width,
        height: image.height,
        pixels: vec![[0; 4]; pixels],
    };
    let mut run_end = vec![0u32; line];
    for stage in 0..4 {
        let (src, dst) = if stage % 2 == 0 {
            (&*image, &mut post)
        } else {
            (&post, &mut *image)
        };
        let horizontal = stage % 2 == 0;
        let (major, minor) = if horizontal {
            (src.height, src.width)
        } else {
            (src.width, src.height)
        };
        let mut cached: Option<([u8; 4], [u8; 4])> = None;
        for outer in 0..major {
            let at = |inner: u32| {
                if horizontal {
                    (outer * src.width + inner) as usize
                } else {
                    (inner * src.width + outer) as usize
                }
            };
            charge(work, u64::from(minor), budget)?;
            run_end[minor as usize - 1] = minor;
            for inner in (0..minor - 1).rev() {
                run_end[inner as usize] = if src.pixels[at(inner)] == src.pixels[at(inner + 1)] {
                    run_end[inner as usize + 1]
                } else {
                    inner + 1
                };
            }
            for inner in 0..minor {
                let index = at(inner);
                let packed = src.pixels[index];
                let info = packed.map(|v| f32::from(v) / 255.);
                let (x, y) = if horizontal {
                    (inner, outer)
                } else {
                    (outer, inner)
                };
                let uv = Vec2::new(
                    (x as f32 + 0.5) / src.width as f32,
                    (y as f32 + 0.5) / src.height as f32,
                );
                let early = stage >= 2
                    && ((info[2] == 0. && info[3] == 0.) || (info[2] == 1. && info[3] == 1.));
                let strength = if stage < 2 {
                    u.amount
                } else {
                    pcf_strength(info, u)
                };
                // Every possible source/noise-offset tap lies inside this
                // interval. Two extra pixels cover float coordinate rounding.
                let radius = (strength[0].abs().max(strength[1].abs()) * 11.5 * minor as f32)
                    .ceil()
                    .min(minor as f32) as u32;
                let radius = radius.saturating_add(2);
                let left = inner.saturating_sub(radius);
                let right = inner.saturating_add(radius).min(minor - 1);
                let constant = early || run_end[left as usize] > right;
                let out = if constant && cached.is_some_and(|(p, _)| p == packed) {
                    charge(work, 2, budget)?;
                    cached.unwrap().1
                } else {
                    charge(work, if early { 2 } else { 49 }, budget)?;
                    let out = pcf_pixel(stage, src, noise, uv, info, constant, u);
                    if constant {
                        cached = Some((packed, out));
                    }
                    out
                };
                if stage == 1 {
                    dst.pixels[index][2] = out[2];
                    dst.pixels[index][3] = out[3];
                } else {
                    dst.pixels[index] = out;
                }
            }
        }
    }
    Ok(())
}
/// Captures and filters each floor once. All output pixels/keys are owned;
/// source meshes, maps, noise and simulation state remain unchanged.
pub fn prepare_wcrc_sunlight(
    input: &WcrcGeometryInput,
    maps: &PreparedRoomMaps,
    sun: DirectionalLighting,
    ultra: bool,
    software_depth: bool,
    budget: LightingBudget,
) -> Result<PreparedWcrcSunlight> {
    let (size, _, _) = validate(input, maps, sun, ultra, software_depth, budget)?;
    let pixels = budget.texture(size[0], size[1])?;
    let count = maps.maps().len();
    let descriptors = count * std::mem::size_of::<PreparedWcrcCapture>();
    budget.bytes(add(add(maps.resident_bytes(), pixels * 8)?, descriptors)?)?;
    let mut floors = Vec::with_capacity(count);
    let (mut bytes, mut work) = (descriptors, 0u64);
    for floor in 0..count {
        let remaining = LightingBudget {
            max_total_bytes: budget
                .max_total_bytes
                .checked_sub(bytes)
                .ok_or(IsoError::Limit("WCRC retained targets"))?,
            max_raster_samples: budget.max_raster_samples.saturating_sub(work),
            ..budget
        };
        let mut capture = capture_wcrc_outside(
            input,
            maps,
            sun,
            ultra,
            software_depth,
            floor as u8,
            remaining,
        )?;
        let mut floor_work = capture.raster_samples;
        let remaining = LightingBudget {
            max_total_bytes: remaining
                .max_total_bytes
                .checked_sub(maps.resident_bytes())
                .ok_or(IsoError::Limit("WCRC blur retained bytes"))?,
            ..remaining
        };
        blur(
            &mut capture.image,
            &input.noise.image,
            maps.width(),
            sun.light.falloff_multiplier,
            &mut floor_work,
            remaining,
        )?;
        let mut filtered_key = [0u8; 64];
        filtered_key[..32].copy_from_slice(&capture.key.0);
        let tag = b"WCRC-PCF-stages-1234-RGBA8-v1";
        filtered_key[32..32 + tag.len()].copy_from_slice(tag);
        capture.key = DerivedKey::new(input.source, maps.source(), 2, &filtered_key);
        capture.raster_samples = floor_work;
        work += floor_work;
        bytes += capture.resident_bytes();
        floors.push(capture);
    }
    let mut key_bytes = [0u8; 160];
    for (i, floor) in floors.iter().enumerate() {
        key_bytes[i * 32..i * 32 + 32].copy_from_slice(&floor.key.0);
    }
    Ok(PreparedWcrcSunlight {
        key: DerivedKey::new(input.source, maps.source(), 1, &key_bytes[..count * 32]),
        room_map_key: maps.key(),
        sunlight: sun,
        ultra,
        software_depth,
        floors,
        resident_bytes: bytes,
        raster_samples: work,
    })
}
/// LightMap2D.SSAASample's four point samples, including its positive half-texel
/// offset. This convention differs from averaging symmetrically around UV.
pub(super) fn ssaa(image: &RgbaImage, uv: Vec2) -> [f32; 2] {
    let texel = Vec2::new(1. / image.width as f32, 1. / image.height as f32);
    let uv = uv + texel * 0.5;
    let mut out = [0.; 2];
    for delta in [
        Vec2::ZERO,
        Vec2::new(texel.x, 0.),
        Vec2::new(0., texel.y),
        texel,
    ] {
        let p = point(image, uv + delta, false);
        out[0] += p[0] * 0.25;
        out[1] += p[1] * 0.25;
    }
    out
}
