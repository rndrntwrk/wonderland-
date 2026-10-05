use crate::*;
use wonderland_render_core::{AssetKey, EntityRef, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LightingResources {
    pub ambient: Option<AssetKey>,
    pub advanced: Option<AssetKey>,
    pub direction: Option<AssetKey>,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightAtlasLayout {
    pub width: u32,
    pub height: u32,
    pub pixels_per_tile: u32,
    pub wall_shadow: [u32; 2],
    pub object_shadow: [u32; 2],
    pub color_atlas: [u32; 2],
    pub direction_atlas: [u32; 2],
}
impl LightAtlasLayout {
    /// `max_pixels` is a per-texture limit. Combined residency belongs to core.
    pub fn new(
        width: u32,
        height: u32,
        ultra: bool,
        software_depth: bool,
        max_pixels: usize,
    ) -> Result<Self> {
        if width < 3 || height < 3 {
            return Err(IsoError::Invalid("lighting lot dimensions"));
        }
        let ultra = ultra && !(software_depth && width > 64);
        let r = if ultra { 16 } else { 8 };
        let dimension = |n: u32, factor: u32| {
            (n - 1)
                .checked_mul(factor)
                .ok_or(IsoError::Limit("light texture dimension"))
        };
        let wall = [dimension(width, r)?, dimension(height, r)?];
        let object = [
            dimension(width, if ultra { r * 2 } else { r })?,
            dimension(height, if ultra { r * 2 } else { r })?,
        ];
        let color = [dimension(width, r * 3)?, dimension(height, r * 2)?];
        let direction = [dimension(width, 12)?, dimension(height, 8)?];
        for size in [wall, object, color, direction] {
            let count = (size[0] as usize)
                .checked_mul(size[1] as usize)
                .ok_or(IsoError::Limit("light texture pixels"))?;
            if count > max_pixels {
                return Err(IsoError::Limit("light texture pixels"));
            }
        }
        Ok(Self {
            width,
            height,
            pixels_per_tile: r,
            wall_shadow: wall,
            object_shadow: object,
            color_atlas: color,
            direction_atlas: direction,
        })
    }
    pub fn floor_slot(self, floor: u8) -> Result<[u32; 2]> {
        if floor >= 6 {
            return Err(IsoError::Invalid("light atlas floor"));
        }
        Ok([u32::from(floor % 3), u32::from(floor / 3)])
    }
    pub fn scissor_origin(self, floor: u8) -> Result<[u32; 2]> {
        let slot = self.floor_slot(floor)?;
        Ok([slot[0] * self.wall_shadow[0], slot[1] * self.wall_shadow[1]])
    }
    pub fn world_to_atlas(self) -> Vec3 {
        Vec3::new(
            1. / (9. * (self.width - 1) as f32),
            1. / 8.85,
            1. / (6. * (self.height - 1) as f32),
        )
    }
    pub fn atlas_coordinates(
        self,
        world: Vec3,
        floor: u8,
        wall_offset: Vec2,
        grass: bool,
    ) -> Result<Vec2> {
        if !world.is_finite() || !wall_offset.is_finite() || self.width < 3 || self.height < 3 {
            return Err(IsoError::Invalid("light atlas coordinates"));
        }
        let slot = self.floor_slot(floor)?;
        let factor = self.world_to_atlas();
        // Preserve the source's shared width-2 wall-offset denominator. Atlas
        // scissor y uses height separately (the explicit nonsquare correction).
        let offset = if grass {
            Vec2::ZERO
        } else {
            Vec2::new(
                -wall_offset.x / ((self.width - 2) as f32 * 3.),
                -wall_offset.y / ((self.width - 2) as f32 * 2.),
            )
        };
        let uv = Vec2::new(
            world.x * factor.x + slot[0] as f32 / 3.,
            world.z * factor.z + slot[1] as f32 / 2.,
        ) + offset;
        if !uv.is_finite() {
            return Err(IsoError::Invalid("light UV overflow"));
        }
        Ok(uv)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlurShadowSample {
    pub average: f32,
    pub spacing: f32,
}
pub fn ultra_floor_shadow(
    samples: [f32; 25],
    width: u32,
    distance_over_radius: f32,
    outdoors_color: bool,
) -> Result<BlurShadowSample> {
    if width == 0
        || !distance_over_radius.is_finite()
        || distance_over_radius < 0.
        || samples
            .iter()
            .any(|x| !x.is_finite() || !(0. ..=1.).contains(x))
    {
        return Err(IsoError::Invalid("ultra floor shadow samples"));
    }
    let min = if outdoors_color {
        1. / (width as f32 * 9.)
    } else {
        0.
    };
    let max = 1. / (width as f32 * 5.);
    let spacing = min * (1. - distance_over_radius) + max * distance_over_radius;
    if !spacing.is_finite() {
        return Err(IsoError::Invalid("ultra blur spacing overflow"));
    }
    Ok(BlurShadowSample {
        average: samples.iter().sum::<f32>() / 25.,
        spacing,
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointSample {
    pub color: [f32; 3],
    pub intensity: f32,
    pub distance_over_radius: f32,
    pub wall_shadow: f32,
    pub floor_shadow: f32,
    pub outdoors_color: bool,
    pub window_ambient: Option<u16>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RoomLightInput {
    pub minimum: [u8; 4],
    pub outside: [u8; 4],
    pub outdoor_contribution: [f32; 4],
    pub points: Vec<PointSample>,
}
/// Shader arithmetic before attachment quantization. Wall/floor shadow samples
/// arrive from an explicit provider/engine; no invented geometry shadow is used.
pub fn evaluate_room_light(input: &RoomLightInput, max_points: usize) -> Result<[f32; 4]> {
    if input.points.len() > max_points {
        return Err(IsoError::Limit("point lights"));
    }
    if input
        .outdoor_contribution
        .iter()
        .any(|x| !x.is_finite() || *x < 0.)
    {
        return Err(IsoError::Invalid("outdoor light contribution"));
    }
    let minimum = input.minimum.map(|v| f32::from(v) / 255.);
    let outside = input.outside.map(|v| f32::from(v) / 255.);
    let mut atlas = [minimum[3]; 4];
    for i in 0..4 {
        atlas[i] += input.outdoor_contribution[i];
    }
    for p in &input.points {
        validate_point(*p)?;
    }
    for outdoors in [true, false] {
        if !outdoors {
            for i in 0..3 {
                atlas[i] *= outside[i];
            }
            atlas[3] *= (outside[0] + outside[1] + outside[2]) / 3.;
        }
        for p in input.points.iter().filter(|p| p.outdoors_color == outdoors) {
            let intensity = p
                .window_ambient
                .map_or(p.intensity, |a| f32::from(a) / 150.);
            if p.window_ambient.is_some() && intensity < 0.2 {
                continue;
            }
            let average = (p.color[0] + p.color[1] + p.color[2]) / 3.;
            let mut color = [p.color[0], p.color[1], p.color[2], average];
            for i in 0..4 {
                color[i] *= intensity * if outdoors { 1. - minimum[i] } else { 0.70 };
            }
            let c = point_light(
                color,
                p.distance_over_radius,
                p.wall_shadow,
                p.floor_shadow,
                [1.; 2],
            );
            for i in 0..4 {
                atlas[i] += c[i];
            }
        }
    }
    if atlas.iter().any(|v| !v.is_finite()) {
        return Err(IsoError::Invalid("light arithmetic overflow"));
    }
    Ok(atlas)
}
fn validate_point(p: PointSample) -> Result<()> {
    if p.color
        .iter()
        .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        || !p.intensity.is_finite()
        || p.intensity < 0.
        || !p.distance_over_radius.is_finite()
        || p.distance_over_radius < 0.
        || ![p.wall_shadow, p.floor_shadow]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    {
        return Err(IsoError::Invalid("point light samples"));
    }
    Ok(())
}
pub fn floor_light_color(
    intensity: [f32; 4],
    minimum: [f32; 4],
    lighting_adjust: f32,
    height_fade: f32,
) -> Result<[f32; 4]> {
    if intensity.iter().any(|x| !x.is_finite() || *x < 0.)
        || minimum
            .iter()
            .any(|x| !x.is_finite() || !(0. ..=1.).contains(x))
        || !lighting_adjust.is_finite()
        || lighting_adjust < 0.
        || !height_fade.is_finite()
        || !(0. ..=1.).contains(&height_fade)
    {
        return Err(IsoError::Invalid("floor light inputs"));
    }
    let avg = (intensity[0] + intensity[1] + intensity[2]) / 3.;
    let shadow = intensity[3] / avg.max(0.0001);
    let shadow = shadow * (1. - height_fade) + height_fade;
    let min_avg = (minimum[0] + minimum[1] + minimum[2]) / 3.;
    let inverse = if min_avg == 1. {
        1.
    } else {
        1. / (1. - min_avg)
    };
    let fraction = (shadow - min_avg) * inverse;
    let target = [
        intensity[0] * lighting_adjust,
        intensity[1] * lighting_adjust,
        intensity[2] * lighting_adjust,
        1.,
    ];
    let mut result = [0.; 4];
    for i in 0..4 {
        result[i] = minimum[i] * (1. - fraction) + target[i] * fraction;
    }
    if result.iter().any(|x| !x.is_finite()) {
        return Err(IsoError::Invalid("floor light arithmetic overflow"));
    }
    Ok(result)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightCluster {
    pub position_sixteenths: Vec2,
    pub radius_sixteenths: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub weight: f32,
    pub outdoors_color: bool,
}
/// Literal legacy compatibility: merged weight remains the first light's weight.
/// This intentionally retains order dependence; it is tested and documented.
pub fn cluster_source_lights(
    lights: &[LightCluster],
    max_lights: usize,
) -> Result<Vec<LightCluster>> {
    if lights.len() > max_lights {
        return Err(IsoError::Limit("light clusters"));
    }
    for l in lights {
        if !l.position_sixteenths.is_finite()
            || !l.radius_sixteenths.is_finite()
            || l.radius_sixteenths <= 0.
            || !l.intensity.is_finite()
            || l.intensity < 0.
            || !l.weight.is_finite()
            || l.weight <= 0.
            || l.color
                .iter()
                .any(|x| !x.is_finite() || !(0. ..=1.).contains(x))
        {
            return Err(IsoError::Invalid("light cluster"));
        }
    }
    let mut out = lights.to_vec();
    let mut i = 0;
    while i < out.len() {
        let mut j = i + 1;
        while j < out.len() {
            let a = out[i];
            let b = out[j];
            let d = a.position_sixteenths - b.position_sixteenths;
            if a.outdoors_color == b.outdoors_color && d.x * d.x + d.y * d.y <= 1024. {
                let w = a.weight + b.weight;
                let position =
                    a.position_sixteenths * (a.weight / w) + b.position_sixteenths * (b.weight / w);
                let radius = (a.radius_sixteenths + b.radius_sixteenths) * 0.6;
                let intensity = ((a.intensity + b.intensity) * 0.75).min(1.25);
                if !position.is_finite() || !radius.is_finite() || !w.is_finite() {
                    return Err(IsoError::Invalid("cluster arithmetic overflow"));
                }
                let color = [
                    (a.color[0] + b.color[0]).min(1.),
                    (a.color[1] + b.color[1]).min(1.),
                    (a.color[2] + b.color[2]).min(1.),
                    (a.color[3] + b.color[3]).min(1.),
                ];
                out[i] = LightCluster {
                    position_sixteenths: position,
                    radius_sixteenths: radius,
                    intensity,
                    color,
                    ..a
                };
                out.remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    Ok(out)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OccluderInput {
    pub reference: EntityRef,
    pub group_id: u64,
    pub footprint_sixteenths: Rect,
    pub room: u16,
    pub floor: u8,
    pub stationary: bool,
    pub emits_light: bool,
    pub main_source: bool,
}
/// Provider supplies one source LightBounds rectangle per multitile main source,
/// in sixteenths. It is presentation occlusion input, never placement legality.
pub fn prepare_occlusion(
    inputs: &[OccluderInput],
    room: u16,
    floor: u8,
    light_bounds: Option<Rect>,
    max_inputs: usize,
) -> Result<Vec<OccluderInput>> {
    if inputs.len() > max_inputs {
        return Err(IsoError::Limit("occlusion inputs"));
    }
    if floor >= 6 || light_bounds.map_or(false, |b| !valid_rect(b)) {
        return Err(IsoError::Invalid("occlusion selection"));
    }
    let mut groups = std::collections::BTreeSet::new();
    let mut out = vec![];
    for i in inputs {
        if i.reference.generation == 0
            || i.reference.object_id == 0
            || i.group_id == 0
            || i.floor >= 6
            || !valid_rect(i.footprint_sixteenths)
        {
            return Err(IsoError::Invalid("occluder input"));
        }
        if i.room != room || i.floor != floor || !i.stationary || i.emits_light || !i.main_source {
            continue;
        }
        if !groups.insert(i.group_id) {
            return Err(IsoError::Invalid(
                "duplicate multitile occluder main source",
            ));
        }
        if light_bounds.map_or(false, |b| !b.intersects(i.footprint_sixteenths)) {
            continue;
        }
        out.push(*i);
    }
    Ok(out)
}
fn valid_rect(r: Rect) -> bool {
    [r.x, r.y, r.width, r.height, r.x + r.width, r.y + r.height]
        .iter()
        .all(|x| x.is_finite())
        && r.width > 0.
        && r.height > 0.
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRoom {
    pub room: u16,
    pub floor: u8,
    pub priority: u32,
    pub has_walls: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightQueue {
    pub rooms: Vec<DirtyRoom>,
    pub max_rooms: usize,
}
impl LightQueue {
    pub fn invalidate(
        &mut self,
        room: u16,
        floor: u8,
        important: bool,
        has_walls: bool,
    ) -> Result<()> {
        if floor >= 6 {
            return Err(IsoError::Invalid("dirty room floor"));
        }
        if let Some(existing) = self.rooms.iter_mut().find(|r| r.room == room) {
            existing.priority = if important {
                u32::MAX
            } else {
                existing.priority.saturating_mul(2)
            };
            existing.floor = floor;
            existing.has_walls = has_walls;
            return Ok(());
        }
        if self.rooms.len() >= self.max_rooms {
            return Err(IsoError::Limit("dirty room queue"));
        }
        self.rooms.push(DirtyRoom {
            room,
            floor,
            priority: if important { u32::MAX } else { 1 },
            has_walls,
        });
        Ok(())
    }
    pub fn take_source_budget(&mut self, floor_limit: u8) -> Vec<u16> {
        self.rooms.sort_by_key(|r| r.floor);
        let mut processed = 0u32;
        let mut result = vec![];
        self.rooms.retain(|r| {
            if !r.has_walls || r.floor > floor_limit {
                return false;
            }
            if processed >= r.priority {
                return true;
            }
            result.push(r.room);
            if r.priority != u32::MAX {
                processed = processed.saturating_add(1);
            }
            false
        });
        result
    }
}
