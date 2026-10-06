//! Injected UTC environment and explicit unmeasured quality/LOD policies.
use crate::legacy_random::LegacyRandom;
use crate::Error;
use std::sync::Arc;
use wonderland_render_core::{
    cache::DerivedKey, AssetKey, Mat4, Mesh, Vec2, Vec3, Vertex, ViewMode,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherType {
    Rain,
    Snow,
    Hail,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub manual: bool,
    pub kind: WeatherType,
    pub intensity: f32,
    pub thunder: bool,
    pub darken: f32,
}
pub fn decode_weather(word: u16) -> Weather {
    let kind = match (word >> 9) & 3 {
        0 => WeatherType::Rain,
        1 => WeatherType::Snow,
        2 => WeatherType::Hail,
        _ => WeatherType::Unknown,
    };
    let intensity = f32::from(word & 255) / 100.;
    Weather {
        manual: word & (1 << 8) != 0,
        kind,
        intensity,
        thunder: word & (1 << 11) != 0,
        darken: if kind == WeatherType::Rain {
            intensity
        } else {
            0.
        },
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoWeatherTuning {
    Snow,
    LightRain,
    Rain,
}
pub fn automatic_weather(
    utc_seconds_since_2019_01_26: i64,
    tuning: AutoWeatherTuning,
    disabled: bool,
) -> Result<Weather, Error> {
    let previous = utc_seconds_since_2019_01_26
        .checked_sub(3600)
        .ok_or(Error::InvalidInput("UTC weather range"))?;
    let hour = i32::try_from(utc_seconds_since_2019_01_26 / 3600)
        .map_err(|_| Error::InvalidInput("UTC weather seed range"))?;
    let prev = i32::try_from(previous / 3600)
        .map_err(|_| Error::InvalidInput("UTC weather seed range"))?;
    let mode = |seed| {
        if disabled {
            return 0;
        }
        let mut r = LegacyRandom::new(seed);
        let weather = (r.next(6) - 3).max(0);
        (match tuning {
            AutoWeatherTuning::Snow => weather,
            AutoWeatherTuning::LightRain => 3 + (weather - 1).max(0),
            AutoWeatherTuning::Rain => weather + 3,
        }) as usize
    };
    let (cur, last) = (mode(hour), mode(prev));
    let f = (utc_seconds_since_2019_01_26.rem_euclid(3600) as f32 / 150.).min(1.);
    let intensities = [0., 0.25, 1., 0., 0.25, 1.];
    let dark = [0., 0., 0., 0., 0.25, 1.];
    Ok(Weather {
        manual: false,
        kind: if cur >= 3 {
            WeatherType::Rain
        } else {
            WeatherType::Snow
        },
        intensity: intensities[cur] * f + intensities[last] * (1. - f),
        thunder: false,
        darken: dark[cur] * f + dark[last] * (1. - f),
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeOfDay {
    pub outside_rgba: [u8; 4],
    pub sky_gradient: f32,
    pub night: bool,
}
pub fn time_of_day(fraction: f64, darkness: f32) -> Result<TimeOfDay, Error> {
    if !fraction.is_finite() || !darkness.is_finite() || !(0.0..=1.0).contains(&darkness) {
        return Err(Error::InvalidInput("time of day/darkness"));
    }
    let colors: [[u8; 3]; 14] = [
        [62, 87, 152],
        [62, 87, 152],
        [68, 93, 138],
        [87, 87, 87],
        [217, 109, 50],
        [255; 3],
        [255; 3],
        [255; 3],
        [255; 3],
        [255; 3],
        [217, 109, 50],
        [87, 87, 87],
        [68, 93, 138],
        [62, 87, 152],
    ];
    let skies = [
        0.5, 0.5, 0.5, 0.625, 0.75, 0.875, 1., 0., 0., 0., 0.125, 0.25, 0.375, 0.5,
    ];
    let t = fraction.rem_euclid(1.);
    let scaled = (t * 13.).min(13. - f64::EPSILON * 8.);
    let i = (scaled.floor() as usize).min(12);
    let f = (scaled - i as f64) as f32;
    let mut rgba = [255; 4];
    for c in 0..3 {
        let a = (255. * (1. - darkness) + f32::from(colors[i][c]) * darkness) as u8;
        let b = (255. * (1. - darkness) + f32::from(colors[i + 1][c]) * darkness) as u8;
        let byte = (f32::from(a) * (1. - f) + f32::from(b) * f) as u8;
        rgba[c] = ((f32::from(byte) / 255.).powf(2.2) * 255.) as u8;
    }
    let sf = if skies[i] == 1. && skies[i + 1] == 0. {
        0.
    } else {
        f
    };
    Ok(TimeOfDay {
        outside_rgba: rgba,
        sky_gradient: skies[i] * (1. - sf) + skies[i + 1] * sf,
        night: t < 0.25 || t > 0.85,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityTier {
    Low,
    Medium,
    High,
    Ultra,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntiAlias {
    Off,
    Msaa4,
    Ssaa2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppliedAa {
    Off,
    Msaa(u8),
    Ssaa(u8),
}
#[derive(Clone, Copy, Debug)]
pub struct QualityRequest {
    pub lighting: u8,
    pub aa: AntiAlias,
    pub weather: bool,
    pub surrounding_lots: u8,
    pub directional: bool,
    pub complex_shaders: bool,
    pub transitions: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub max_msaa: u8,
    pub ssaa: bool,
    pub depth_stencil: bool,
    pub complex_shaders: bool,
    pub memory_budget_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectLod {
    Hidden,
    Impostor,
    Reduced,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualityPolicy {
    pub lighting: u8,
    pub advanced_lighting: bool,
    pub shadows_3d: bool,
    pub ultra_lighting: bool,
    pub aa: AppliedAa,
    pub weather: bool,
    pub surrounding_lots: u8,
    pub directional: bool,
    pub complex_shaders: bool,
    pub transitions: bool,
    pub tier: QualityTier,
    pub memory_budget_bytes: u64,
}
impl QualityPolicy {
    pub fn object_lod(self, projected_pixels: f32) -> Result<ObjectLod, Error> {
        if !projected_pixels.is_finite() || projected_pixels < 0. {
            return Err(Error::InvalidInput("projected object size"));
        }
        let (full, reduced) = match self.tier {
            QualityTier::Low => (128., 48.),
            QualityTier::Medium => (96., 32.),
            QualityTier::High => (64., 24.),
            QualityTier::Ultra => (32., 12.),
        };
        Ok(if projected_pixels < 2. {
            ObjectLod::Hidden
        } else if projected_pixels < reduced {
            ObjectLod::Impostor
        } else if projected_pixels < full {
            ObjectLod::Reduced
        } else {
            ObjectLod::Full
        })
    }
}
/// Policy budgets are design limits, not measured device-performance claims.
pub fn choose_quality(
    request: QualityRequest,
    caps: Capabilities,
    tier: QualityTier,
    mode: ViewMode,
) -> Result<QualityPolicy, Error> {
    if request.lighting > 3
        || !matches!(caps.max_msaa, 0 | 1 | 2 | 4 | 8 | 16)
        || caps.memory_budget_bytes == 0
    {
        return Err(Error::InvalidInput("quality/capabilities"));
    }
    let cap = match tier {
        QualityTier::Low => 0,
        QualityTier::Medium => 1,
        QualityTier::High => 2,
        QualityTier::Ultra => 3,
    };
    let lighting = request
        .lighting
        .min(cap)
        .min(if caps.depth_stencil { 3 } else { 1 });
    let msaa = |wanted: u8| {
        let samples = wanted.min(caps.max_msaa);
        if samples < 2 {
            AppliedAa::Off
        } else {
            AppliedAa::Msaa(samples)
        }
    };
    let aa = if tier == QualityTier::Low {
        AppliedAa::Off
    } else {
        match request.aa {
            AntiAlias::Off => AppliedAa::Off,
            AntiAlias::Msaa4 => msaa(4),
            AntiAlias::Ssaa2 => {
                if mode != ViewMode::Full3D {
                    msaa(8)
                } else if caps.ssaa && matches!(tier, QualityTier::High | QualityTier::Ultra) {
                    AppliedAa::Ssaa(2)
                } else {
                    msaa(4)
                }
            }
        }
    };
    Ok(QualityPolicy {
        lighting,
        advanced_lighting: lighting > 0,
        shadows_3d: lighting > 1,
        ultra_lighting: lighting > 2,
        aa,
        weather: request.weather,
        surrounding_lots: request.surrounding_lots.min(match tier {
            QualityTier::Low => 0,
            QualityTier::Medium => 1,
            _ => 2,
        }),
        directional: request.directional && lighting > 0,
        complex_shaders: request.complex_shaders
            && caps.complex_shaders
            && matches!(tier, QualityTier::High | QualityTier::Ultra),
        transitions: request.transitions,
        tier,
        memory_budget_bytes: caps.memory_budget_bytes,
    })
}

/// Bounds apply to the whole prepared frame, including all fading emitters and
/// all repeated 2D weather passes. Retained clones are the caller's residency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentBudget {
    pub max_particles: usize,
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_indoors_pixels: usize,
    pub max_total_bytes: usize,
}
impl Default for EnvironmentBudget {
    fn default() -> Self {
        Self {
            max_particles: 65536,
            max_vertices: 1_048_576,
            max_indices: 1_572_864,
            max_indoors_pixels: 512 * 512,
            max_total_bytes: 128 * 1024 * 1024,
        }
    }
}
impl EnvironmentBudget {
    fn geometry(
        self,
        particles: usize,
        vertices: usize,
        indices: usize,
        bytes: usize,
    ) -> Result<(), Error> {
        if particles > self.max_particles
            || vertices > self.max_vertices
            || indices > self.max_indices
            || bytes > self.max_total_bytes
            || vertices > u32::MAX as usize
        {
            Err(Error::BudgetExceeded("environment frame resources"))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSkyDome {
    key: DerivedKey,
    mesh: Mesh,
}
impl PreparedSkyDome {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }
}
/// AbstractSkyDome.BuildSkyDome: 65 subdivisions, source pole/ring ordering,
/// gradient half-column offset and non-linear vertical gradient lookup.
pub fn build_sky_dome(
    day_fraction: f64,
    gradient_height: u32,
    budget: EnvironmentBudget,
) -> Result<PreparedSkyDome, Error> {
    if gradient_height == 0 || gradient_height > 65536 {
        return Err(Error::InvalidInput("sky gradient height"));
    }
    let time = time_of_day(day_fraction, 1.)?;
    let (vertex_count, index_count) = (4225, 24765);
    budget.geometry(
        0,
        vertex_count,
        index_count,
        vertex_count * std::mem::size_of::<Vertex>() + index_count * 4,
    )?;
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(vertex_count),
        indices: Vec::with_capacity(index_count),
    };
    let sky_pos = time.sky_gradient + 1. / 16.;
    let gap = 1. / gradient_height as f32;
    let range = 1. - gap;
    let add = |position: Vec3, uv: Vec2| Vertex {
        position,
        normal: -position,
        uv,
        color: [1.; 4],
    };
    mesh.vertices.push(add(Vec3::Y, Vec2::new(sky_pos, gap)));
    let (mut previous_start, mut previous_length) = (0u32, 1u32);
    for y in 1..65 {
        let start = mesh.vertices.len() as u32;
        let angle_y = (std::f64::consts::PI as f32) * y as f32 / 64.;
        let radius = f64::from(angle_y).sin() as f32;
        let height = f64::from(angle_y).cos();
        let tpos = if height > 0. {
            0.9 - height.sqrt() * f64::from(0.9 * range)
        } else {
            0.9 + (-height).sqrt() * f64::from(0.1 * range)
        } as f32;
        for x in 0..66 {
            let angle_x = (std::f64::consts::PI as f32) * x as f32 * 2. / 65.;
            let position = Vec3::new(
                f64::from(angle_x).sin() as f32 * radius,
                height as f32,
                f64::from(angle_x).cos() as f32 * radius,
            );
            mesh.vertices.push(add(position, Vec2::new(sky_pos, tpos)));
            let vi = mesh.vertices.len() as u32;
            if x < 65 {
                if y != 1 {
                    mesh.indices.extend_from_slice(&[
                        previous_start + x % previous_length,
                        previous_start + (x + 1) % previous_length,
                        vi - 1,
                    ]);
                }
                mesh.indices.extend_from_slice(&[
                    previous_start + (x + 1) % previous_length,
                    vi,
                    vi - 1,
                ]);
            }
        }
        previous_start = start;
        previous_length = 66;
    }
    let mut params = b"source-sky-dome-v1\0".to_vec();
    params.extend_from_slice(&time.sky_gradient.to_le_bytes());
    params.extend_from_slice(&gradient_height.to_le_bytes());
    let key = DerivedKey::new(AssetKey([0; 32]), AssetKey([0; 32]), 1, &params);
    Ok(PreparedSkyDome { key, mesh })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunState {
    pub sun_vector: Vec3,
    pub light_position_sixteenths: Vec2,
    pub shadow_direction: Vec2,
    pub light_vector: Vec3,
    pub falloff_multiplier: f32,
    pub shadow_multiplier: f32,
    pub night: bool,
    /// A finite extrusion bound at the horizon; shadow strength is already fading to zero.
    pub horizon_clamped: bool,
}
/// LMapBatch.BuildOutdoorsLight. Finale/custom sun-time bias must be applied to
/// the injected day fraction by its provider; no wall clock is read here.
pub fn source_sun(day_fraction: f64) -> Result<SunState, Error> {
    if !day_fraction.is_finite() {
        return Err(Error::InvalidInput("solar time"));
    }
    let t = day_fraction.rem_euclid(1.);
    let (mut phase, night) = if t < 0.25 {
        ((0.15 + t) * 0.5 / 0.4, true)
    } else if t > 0.85 {
        ((t - 0.85) * 0.5 / 0.4, true)
    } else {
        ((t - 0.25) * 0.5 / 0.6, false)
    };
    let rotate_y = |p: Vec3, a: f32| {
        let (s, c) = (f64::from(a).sin() as f32, f64::from(a).cos() as f32);
        Vec3::new(p.x * c + p.z * s, p.y, -p.x * s + p.z * c)
    };
    let a = ((phase + 0.5) * std::f64::consts::TAU) as f32;
    let mut p = rotate_y(Vec3::new(0., 0., -3000.), a);
    let a = std::f64::consts::FRAC_PI_4 as f32;
    let (s, c) = (f64::from(a).sin() as f32, f64::from(a).cos() as f32);
    p = Vec3::new(p.x * c - p.y * s, p.x * s + p.y * c, p.z);
    p = rotate_y(p, (std::f64::consts::PI * 0.3) as f32);
    p.y = p.y.abs();
    let sun = p.normalize_or_zero();
    let light_position = Vec2::new(p.z, -p.x);
    let length = (light_position.x * light_position.x + light_position.y * light_position.y).sqrt();
    let shadow_direction = light_position * (-1. / length);
    let raw = (sun.x * sun.x + sun.z * sun.z).sqrt() / sun.y;
    let falloff = raw.min(10_000.);
    if phase > 0.25 {
        phase = 0.5 - phase;
    }
    let strength = ((phase.abs() * 20.) as f32).min(1.) * if night { 1.33 } else { 1. };
    Ok(SunState {
        sun_vector: sun,
        light_position_sixteenths: light_position,
        shadow_direction,
        light_vector: Vec3::new(sun.z, 1., -sun.x),
        falloff_multiplier: falloff,
        shadow_multiplier: strength,
        night,
        horizon_clamped: raw > 10_000.,
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub fog_color: [f32; 3],
    pub fog_distance: f32,
    pub sky_opacity: f32,
    pub outside_tint: [u8; 4],
}
/// WeatherController.Update and AbstractSkyDome.Draw. Clamp the negative base
/// before fractional gamma power, avoiding the old dark-color NaN.
pub fn atmosphere(outside: [u8; 4], weather: Weather) -> Result<Atmosphere, Error> {
    validate_weather(weather)?;
    let intensity = weather.intensity.min(1.);
    let tint = [128. / 255., 192. / 255., 1.];
    let mut fog = [0.; 3];
    let mut outside_tint = [255; 4];
    for i in 0..3 {
        let srgb = (f32::from(outside[i]) / 255.).powf(1. / 2.2);
        let dark = (srgb - 0.175).max(0.).powf(2.2);
        fog[i] = dark * tint[i] * (1. - intensity * 0.75) + srgb * (intensity * 0.75);
        outside_tint[i] =
            (255. + ([159., 164., 181.][i] - 255.) * weather.darken).clamp(0., 255.) as u8;
    }
    Ok(Atmosphere {
        fog_color: fog,
        fog_distance: intensity * (15. * 75.) + (1. - intensity) * (300. * 75.),
        sky_opacity: 1. - intensity.sqrt() * 0.75,
        outside_tint,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentIdentity {
    pub lot_id: u64,
    pub epoch: u64,
    pub content: AssetKey,
    pub device_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherZoom {
    Near,
    Medium,
    Far,
}
impl WeatherZoom {
    fn scale(self) -> usize {
        match self {
            Self::Near => 1,
            Self::Medium => 2,
            Self::Far => 4,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherFrameInput {
    pub identity: EnvironmentIdentity,
    pub presentation_seconds: f64,
    /// Read-only accepted simulation speed. It scales weather time, never ticks the VM.
    pub sim_speed: f32,
    pub weather: Weather,
    pub enabled: bool,
    pub mode: ViewMode,
    pub zoom: WeatherZoom,
    pub camera_id: u64,
    pub camera_position: Vec3,
    /// Rotation only; no camera translation or scale.
    pub inverse_view_rotation: Mat4,
    pub center_tile: Vec2,
    pub base_altitude_tiles: f32,
    /// One-based selected floor.
    pub level: u8,
    pub stories: u8,
    pub width: u32,
    pub height: u32,
    /// Source Blueprint.GetIndoors Alpha8 map, width*height bytes.
    pub indoors: Vec<u8>,
    pub outside_color: [u8; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherSeed {
    pub position: Vec3,
    pub variation: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherVolume {
    pub min: Vec3,
    pub max: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherUniforms {
    pub time: f32,
    pub time_rate: f32,
    pub translation: Vec3,
    pub camera_velocity: Vec3,
    pub inverse_rotation: Mat4,
    pub inverse_xz_rotation: Mat4,
    pub parameters: [[f32; 4]; 4],
    pub color: [f32; 4],
    pub sub_color: [f32; 4],
    pub clip_level: f32,
    pub level: f32,
    pub base_altitude: f32,
    pub blueprint_size: Vec2,
    pub stories: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherBatch {
    kind: WeatherType,
    seeds: Arc<[WeatherSeed]>,
    mesh: Mesh,
    uniforms: WeatherUniforms,
}
impl WeatherBatch {
    pub fn kind(&self) -> WeatherType {
        self.kind
    }
    pub fn seeds(&self) -> &[WeatherSeed] {
        &self.seeds
    }
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    pub fn uniforms(&self) -> WeatherUniforms {
        self.uniforms
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedWeatherFrame {
    key: DerivedKey,
    batches: Arc<[WeatherBatch]>,
    indoors: Arc<[u8]>,
    atmosphere: Atmosphere,
    resident_bytes: usize,
    particle_count: usize,
}
impl PreparedWeatherFrame {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn batches(&self) -> &[WeatherBatch] {
        &self.batches
    }
    pub fn indoors(&self) -> &[u8] {
        &self.indoors
    }
    pub fn atmosphere(&self) -> Atmosphere {
        self.atmosphere
    }
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }
}
#[derive(Clone)]
struct WeatherEmitter {
    kind: WeatherType,
    intensity: f32,
    time: f64,
    fade_in: Option<f64>,
    fade_out: Option<f64>,
    seeds: Arc<[WeatherSeed]>,
    volume: WeatherVolume,
    seed: i32,
}
#[derive(Clone)]
pub struct WeatherPresentation {
    seed: i32,
    identity: Option<EnvironmentIdentity>,
    last_seconds: Option<f64>,
    serial: u64,
    current: Option<WeatherEmitter>,
    previous: Option<WeatherEmitter>,
    last_camera: Option<(u64, ViewMode, WeatherZoom, Vec3)>,
    last_result: Option<(DerivedKey, PreparedWeatherFrame)>,
}
impl WeatherPresentation {
    pub fn new(seed: i32) -> Self {
        Self {
            seed,
            identity: None,
            last_seconds: None,
            serial: 0,
            current: None,
            previous: None,
            last_camera: None,
            last_result: None,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }
    /// Prepare is transactional: invalid input, stale time, or a combined budget
    /// failure cannot consume a seed, alter a camera history, or replace a frame.
    pub fn prepare(
        &mut self,
        input: &WeatherFrameInput,
        budget: EnvironmentBudget,
    ) -> Result<PreparedWeatherFrame, Error> {
        validate_weather_frame(input, budget)?;
        if input.enabled && input.weather.intensity > 0.01 {
            let count = (12500. * input.weather.intensity) as usize;
            let repetitions = if input.mode == ViewMode::Full3D {
                1
            } else {
                input.zoom.scale()
            };
            budget.geometry(
                count,
                count * 4 * repetitions,
                count * 6 * repetitions,
                count * (16 + (4 * std::mem::size_of::<Vertex>() + 24) * repetitions)
                    + input.indoors.len(),
            )?;
        }
        let input_key = weather_input_key(input, self.seed);
        if let Some((key, result)) = &self.last_result {
            if *key == input_key {
                budget.geometry(
                    result.particle_count,
                    result.batches.iter().map(|b| b.mesh.vertices.len()).sum(),
                    result.batches.iter().map(|b| b.mesh.indices.len()).sum(),
                    result.resident_bytes,
                )?;
                return Ok(result.clone());
            }
        }
        let mut next = self.clone();
        let result = next.prepare_inner(input, budget, input_key)?;
        *self = next;
        Ok(result)
    }
    fn prepare_inner(
        &mut self,
        input: &WeatherFrameInput,
        budget: EnvironmentBudget,
        input_key: DerivedKey,
    ) -> Result<PreparedWeatherFrame, Error> {
        if self.identity != Some(input.identity) {
            self.reset();
            self.identity = Some(input.identity);
        }
        let elapsed = if let Some(last) = self.last_seconds {
            if input.presentation_seconds < last {
                return Err(Error::InvalidInput("backwards presentation time"));
            }
            input.presentation_seconds - last
        } else {
            0.
        };
        let delta = elapsed * f64::from(input.sim_speed) * 0.001;
        for emitter in [&mut self.current, &mut self.previous]
            .into_iter()
            .flatten()
        {
            emitter.time += delta;
            if !emitter.time.is_finite() {
                return Err(Error::InvalidInput("weather time overflow"));
            }
        }
        let now = input.presentation_seconds;
        if self
            .previous
            .as_ref()
            .and_then(|e| e.fade_out)
            .is_some_and(|start| now - start >= 1. / 0.15)
        {
            self.previous = None;
        }
        let desired = if input.weather.kind == WeatherType::Snow {
            WeatherType::Snow
        } else {
            WeatherType::Rain
        };
        let enabled = input.enabled && input.weather.intensity > 0.01;
        let volume = weather_volume(input.mode, input.zoom);
        if !enabled {
            if let Some(mut old) = self.current.take() {
                old.fade_out = Some(now);
                self.previous = Some(old);
            }
        } else {
            let replace = self.current.as_ref().map_or(true, |old| {
                old.kind != desired
                    || (old.intensity - input.weather.intensity).abs() > 0.04
                    || old.time > 100.
            });
            if replace {
                if let Some(mut old) = self.current.take() {
                    old.fade_out = Some(now);
                    self.previous = Some(old);
                }
                let count = (12500. * input.weather.intensity) as usize;
                if count > budget.max_particles {
                    return Err(Error::BudgetExceeded("weather particle count"));
                }
                let seed = self.seed.wrapping_add(self.serial as i32);
                self.serial = self
                    .serial
                    .checked_add(1)
                    .ok_or(Error::BudgetExceeded("weather generation"))?;
                self.current = Some(WeatherEmitter {
                    kind: desired,
                    intensity: input.weather.intensity,
                    time: 0.,
                    fade_in: self.previous.as_ref().map(|_| now),
                    fade_out: None,
                    seeds: seed_weather(volume, count, seed),
                    volume,
                    seed,
                });
            }
        }
        // Rebuild stable seeded positions after camera mode/zoom changes; never reuse the old volume.
        for emitter in [&mut self.current, &mut self.previous]
            .into_iter()
            .flatten()
        {
            if emitter.volume != volume {
                emitter.seeds = seed_weather(volume, emitter.seeds.len(), emitter.seed);
                emitter.volume = volume;
            }
        }
        let cam3d = input.mode == ViewMode::Full3D;
        let forward = input.inverse_view_rotation.transform_vector3(Vec3::Z);
        let translation = if cam3d {
            (input.camera_position - forward * 20. + Vec3::new(volume.max.x, 0., volume.max.z)) * 2.
        } else {
            Vec3::new(
                input.center_tile.x * 3. + volume.max.x,
                input.base_altitude_tiles * 3.,
                input.center_tile.y * 3. + volume.max.z,
            ) * 2.
        };
        let velocity = match self.last_camera {
            Some((id, mode, zoom, last))
                if id == input.camera_id && mode == input.mode && zoom == input.zoom && cam3d =>
            {
                translation - last
            }
            _ => Vec3::ZERO,
        };
        let repetitions = if cam3d { 1 } else { input.zoom.scale() };
        let count = self.current.as_ref().map_or(0, |e| e.seeds.len())
            + self.previous.as_ref().map_or(0, |e| e.seeds.len());
        let vertices = count
            .checked_mul(4)
            .and_then(|n| n.checked_mul(repetitions))
            .ok_or(Error::BudgetExceeded("weather vertices"))?;
        let indices = count
            .checked_mul(6)
            .and_then(|n| n.checked_mul(repetitions))
            .ok_or(Error::BudgetExceeded("weather indices"))?;
        let bytes = vertices
            .checked_mul(std::mem::size_of::<Vertex>())
            .and_then(|n| n.checked_add(indices.checked_mul(4)?))
            .and_then(|n| n.checked_add(count.checked_mul(std::mem::size_of::<WeatherSeed>())?))
            .and_then(|n| n.checked_add(input.indoors.len()))
            .ok_or(Error::BudgetExceeded("weather bytes"))?;
        budget.geometry(
            count,
            vertices,
            indices,
            bytes
                .checked_mul(2)
                .ok_or(Error::BudgetExceeded("weather staging bytes"))?,
        )?;
        let mut batches = Vec::new();
        for emitter in [&self.previous, &self.current].into_iter().flatten() {
            let fade = if let Some(start) = emitter.fade_out {
                1. - ((now - start) * 0.15).clamp(0., 1.) as f32
            } else if let Some(start) = emitter.fade_in {
                1. - (1. - ((now - start) * 0.15).clamp(0., 1.) as f32).powi(2)
            } else {
                1.
            };
            for repeat in 0..repetitions {
                let uniforms =
                    weather_uniforms(input, emitter, translation, velocity, fade, repeat)?;
                let mesh = execute_weather_vertices(emitter.kind, &emitter.seeds, uniforms)?;
                batches.push(WeatherBatch {
                    kind: emitter.kind,
                    seeds: emitter.seeds.clone(),
                    mesh,
                    uniforms,
                });
            }
        }
        let mut params = input_key.0.to_vec();
        for batch in &batches {
            params.extend_from_slice(&batch.uniforms.time.to_le_bytes());
            params.extend_from_slice(&batch.uniforms.color[3].to_le_bytes());
            for v in &batch.mesh.vertices {
                for f in [v.position.x, v.position.y, v.position.z] {
                    params.extend_from_slice(&f.to_le_bytes());
                }
            }
        }
        // Param staging is bounded separately from the retained geometry.
        budget.geometry(
            count,
            vertices,
            indices,
            bytes
                .checked_add(params.len())
                .ok_or(Error::BudgetExceeded("weather hash bytes"))?,
        )?;
        let key = DerivedKey::new(input.identity.content, input.identity.content, 1, &params);
        let result = PreparedWeatherFrame {
            key,
            batches: batches.into(),
            indoors: input.indoors.clone().into(),
            atmosphere: atmosphere(input.outside_color, input.weather)?,
            resident_bytes: bytes,
            particle_count: count,
        };
        self.last_seconds = Some(now);
        self.last_camera = Some((input.camera_id, input.mode, input.zoom, translation));
        self.last_result = Some((input_key, result.clone()));
        Ok(result)
    }
}
fn validate_weather(weather: Weather) -> Result<(), Error> {
    if !weather.intensity.is_finite()
        || !(0. ..=2.55).contains(&weather.intensity)
        || !weather.darken.is_finite()
        || !(0. ..=2.55).contains(&weather.darken)
    {
        Err(Error::InvalidInput("weather intensity/darken"))
    } else {
        Ok(())
    }
}
fn validate_weather_frame(
    input: &WeatherFrameInput,
    budget: EnvironmentBudget,
) -> Result<(), Error> {
    validate_weather(input.weather)?;
    let count = (input.width as usize)
        .checked_mul(input.height as usize)
        .ok_or(Error::BudgetExceeded("weather indoors size"))?;
    if input.width < 1
        || input.height < 1
        || input.width > 65536
        || input.height > 65536
        || count > budget.max_indoors_pixels
        || input.indoors.len() != count
    {
        return Err(Error::InvalidInput("weather indoors map"));
    }
    if input.identity.lot_id == 0
        || input.identity.epoch == 0
        || input.identity.device_generation == 0
        || input.camera_id == 0
        || !input.presentation_seconds.is_finite()
        || input.presentation_seconds < 0.
        || input.presentation_seconds > 1e12
        || !input.sim_speed.is_finite()
        || !(0. ..=1000.).contains(&input.sim_speed)
        || !input.camera_position.is_finite()
        || !input.center_tile.is_finite()
        || !input.base_altitude_tiles.is_finite()
        || input.base_altitude_tiles.abs() > 1_000_000.
        || input.camera_position.length() > 1_000_000.
        || input.center_tile.x.abs() > 1_000_000.
        || input.center_tile.y.abs() > 1_000_000.
        || !(1..=5).contains(&input.stories)
        || input.level == 0
        || input.level > input.stories
        || !input.inverse_view_rotation.is_finite()
    {
        return Err(Error::InvalidInput("weather frame"));
    }
    let m = input.inverse_view_rotation;
    if m.cols[3] != [0., 0., 0., 1.] || [m.cols[0][3], m.cols[1][3], m.cols[2][3]] != [0.; 3] {
        return Err(Error::InvalidInput("weather camera rotation"));
    }
    let axes = [
        m.transform_vector3(Vec3::X),
        m.transform_vector3(Vec3::Y),
        m.transform_vector3(Vec3::Z),
    ];
    if axes.iter().any(|a| (a.length() - 1.).abs() > 0.0001)
        || axes[0].dot(axes[1]).abs() > 0.0001
        || axes[0].dot(axes[2]).abs() > 0.0001
        || axes[1].dot(axes[2]).abs() > 0.0001
    {
        return Err(Error::InvalidInput("weather camera rotation"));
    }
    budget.geometry(0, 0, 0, count)?;
    Ok(())
}
fn weather_input_key(input: &WeatherFrameInput, seed: i32) -> DerivedKey {
    let mut bytes = b"source-weather-frame-v1\0".to_vec();
    for v in [
        input.identity.lot_id,
        input.identity.epoch,
        input.identity.device_generation,
        input.camera_id,
    ] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&seed.to_le_bytes());
    bytes.extend_from_slice(&input.presentation_seconds.to_le_bytes());
    bytes.extend_from_slice(&[
        input.enabled as u8,
        match input.mode {
            ViewMode::Full2D => 0,
            ViewMode::Hybrid2D => 1,
            ViewMode::Full3D => 2,
        },
        input.zoom.scale() as u8,
        input.level,
        input.stories,
        match input.weather.kind {
            WeatherType::Rain => 0,
            WeatherType::Snow => 1,
            WeatherType::Hail => 2,
            WeatherType::Unknown => 3,
        },
        input.weather.manual as u8,
        input.weather.thunder as u8,
    ]);
    for v in [
        input.sim_speed,
        input.weather.intensity,
        input.weather.darken,
        input.camera_position.x,
        input.camera_position.y,
        input.camera_position.z,
        input.center_tile.x,
        input.center_tile.y,
        input.base_altitude_tiles,
    ]
    .into_iter()
    .chain(input.inverse_view_rotation.cols.into_iter().flatten())
    {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&input.width.to_le_bytes());
    bytes.extend_from_slice(&input.height.to_le_bytes());
    bytes.extend_from_slice(&input.outside_color);
    bytes.extend_from_slice(&input.indoors);
    DerivedKey::new(input.identity.content, input.identity.content, 1, &bytes)
}
fn weather_volume(mode: ViewMode, zoom: WeatherZoom) -> WeatherVolume {
    if mode == ViewMode::Full3D {
        WeatherVolume {
            min: Vec3::new(-50., -50., -50.),
            max: Vec3::new(50., 50., 50.),
        }
    } else {
        let size = 100. * zoom.scale() as f32;
        WeatherVolume {
            min: Vec3::new(-size, 0., -size),
            max: Vec3::new(size, 2.95 * 3. * 5. * 2., size),
        }
    }
}
fn seed_weather(volume: WeatherVolume, count: usize, seed: i32) -> Arc<[WeatherSeed]> {
    let mut random = LegacyRandom::new(seed);
    let range = volume.max - volume.min;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let position = Vec3::new(
            (f64::from(volume.min.x) + random.unit() * f64::from(range.x)) as f32,
            (f64::from(volume.min.y) + random.unit() * f64::from(range.y)) as f32,
            (f64::from(volume.min.z) + random.unit() * f64::from(range.z)) as f32,
        );
        result.push(WeatherSeed {
            position,
            variation: random.unit() as f32,
        });
    }
    result.into()
}
fn weather_uniforms(
    input: &WeatherFrameInput,
    emitter: &WeatherEmitter,
    translation: Vec3,
    velocity: Vec3,
    fade: f32,
    repeat: usize,
) -> Result<WeatherUniforms, Error> {
    let cam3d = input.mode == ViewMode::Full3D;
    let volume = emitter.volume;
    let size = volume.max - volume.min;
    let rain = emitter.kind == WeatherType::Rain;
    let speed = velocity.length();
    let opacity = if rain {
        (if speed == 0. {
            1.
        } else {
            (3. / speed + 0.001).min(1.)
        }) / input.sim_speed.max(1.).sqrt()
    } else {
        1.
    };
    let tint = ((255. * opacity) as u8) as f32 / 255.;
    let mut inv_xz = Mat4::IDENTITY;
    if rain && cam3d {
        let mut rotation = input
            .inverse_view_rotation
            .inverse()
            .ok_or(Error::InvalidInput("weather rotation inverse"))?;
        rotation.cols[1] = [0., 1., 0., 0.];
        inv_xz = rotation
            .inverse()
            .ok_or(Error::InvalidInput("weather XZ billboard singular"))?;
    }
    let inverse_rotation = input.inverse_view_rotation;
    let p1 = [
        volume.min.y,
        size.y,
        if rain { 0.20 } else { 1.33 },
        if rain { 0.01 } else { 0.2 },
    ];
    let p2 = [
        30.,
        10.,
        if rain { 10. } else { 12.5 },
        if rain {
            if cam3d {
                0.3
            } else {
                1.
            }
        } else {
            100.
        },
    ];
    let uniforms = WeatherUniforms {
        time: emitter.time as f32 + repeat as f32,
        // Source default refresh (60 Hz) fixes the visual shutter/fade rate; render sampling never changes the VM's 30 Hz timeline.
        time_rate: input.sim_speed.max(1.) * 0.001 / 60.,
        translation,
        camera_velocity: velocity,
        inverse_rotation,
        inverse_xz_rotation: inv_xz,
        parameters: [
            p1,
            p2,
            [volume.min.x, size.x, volume.min.z, size.z],
            [if cam3d { 0.66 } else { 0.8 }, 0., 0., 0.],
        ],
        color: [tint * fade; 4],
        sub_color: if rain {
            input
                .outside_color
                .map(|v| f32::from(v) / 255. * 0.6 * opacity)
        } else {
            [0.; 4]
        },
        clip_level: if cam3d {
            f32::MAX
        } else {
            f32::from(input.level)
        },
        level: f32::from((input.level + 1).min(input.stories)) - 0.999,
        base_altitude: input.base_altitude_tiles * 3.,
        blueprint_size: Vec2::new(input.width as f32 * 3., input.height as f32 * 3.),
        stories: f32::from(input.stories + 1),
    };
    if !uniforms.time.is_finite() || !translation.is_finite() || !velocity.is_finite() {
        return Err(Error::InvalidInput("weather uniform overflow"));
    }
    Ok(uniforms)
}
/// ParticleShader.fx SnowVS/RainVS execution, with source signed remainder,
/// 0.5 billboard scale, floor clip factor, and ModelPos = realPos/2 convention.
/// The source adds two homogeneous positions, so each returned raw XYZ has
/// W = 2. GPU consumers must project `vec4(position, 2)`, or divide XYZ by two
/// before projecting with W = 1. Fragment ModelPos uses the divided position.
pub fn execute_weather_vertices(
    kind: WeatherType,
    seeds: &[WeatherSeed],
    u: WeatherUniforms,
) -> Result<Mesh, Error> {
    if !matches!(kind, WeatherType::Rain | WeatherType::Snow) || seeds.len() > 65536 {
        return Err(Error::InvalidInput("weather particle mode/count"));
    }
    if u.parameters
        .iter()
        .flatten()
        .chain(u.color.iter())
        .chain(u.sub_color.iter())
        .any(|v| !v.is_finite())
        || !u.time.is_finite()
        || u.time < 0.
        || !u.time_rate.is_finite()
        || u.time_rate < 0.
        || u.parameters[0][1] <= 0.
        || u.parameters[0][2] <= u.parameters[0][3].abs()
        || u.parameters[2][1] <= 0.
        || u.parameters[2][3] <= 0.
        || !u.translation.is_finite()
        || !u.camera_velocity.is_finite()
        || !u.inverse_rotation.is_finite()
        || !u.inverse_xz_rotation.is_finite()
        || !u.clip_level.is_finite()
        || !u.base_altitude.is_finite()
    {
        return Err(Error::InvalidInput("weather vertex uniforms"));
    }
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(seeds.len() * 4),
        indices: Vec::with_capacity(seeds.len() * 6),
    };
    for seed in seeds {
        if !seed.position.is_finite()
            || seed.position.length() > 1_000_000.
            || !seed.variation.is_finite()
            || !(0. ..=1.).contains(&seed.variation)
        {
            return Err(Error::InvalidInput("weather seed"));
        }
        let p = seed.position;
        let params = u.parameters;
        let sine = (p.y * 1000.).sin();
        let fall = params[0][2] + sine * params[0][3];
        let repeat = fall / params[0][1];
        let real = (u.time / repeat) % 1.;
        let phase = (p.y - (params[0][0] + u.translation.y)) / params[0][1] - real;
        let fraction = phase - phase.floor();
        let y = params[0][0] + fraction * params[0][1];
        let wind = Vec2::new(
            params[1][0] + (p.x * 1000.).sin() * params[1][2],
            params[1][1] + (p.z * 1000.).sin() * params[1][2],
        );
        let xbase = params[2][0] + u.translation.x;
        let zbase = params[2][2] + u.translation.z;
        let x = (p.x + real * wind.x - xbase) % params[2][1] + xbase;
        let z = (p.z + real * wind.y - zbase) % params[2][3] + zbase;
        let center = Vec3::new(x, y + u.translation.y, z);
        let base = mesh.vertices.len() as u32;
        for (corner, uv) in [
            (Vec3::new(-0.5, -0.5, 0.), Vec2::new(0., 0.)),
            (Vec3::new(-0.5, 0.5, 0.), Vec2::new(0., 1.)),
            (Vec3::new(0.5, 0.5, 0.), Vec2::new(1., 1.)),
            (Vec3::new(0.5, -0.5, 0.), Vec2::new(1., 0.)),
        ] {
            let position = if kind == WeatherType::Snow {
                let angle = sine * params[1][3] * real;
                let (s, c) = angle.sin_cos();
                let rotated = Vec3::new(
                    corner.x * c + corner.y * s,
                    corner.y * c - corner.x * s,
                    corner.z,
                );
                center
                    + u.inverse_rotation
                        .transform_vector3(rotated * ((sine * 0.15 + 1.) * params[3][0]))
                        * 0.5
            } else {
                let step = u.time_rate / repeat;
                let delta = Vec3::new(
                    wind.x * step / -2.,
                    params[0][1] * step,
                    wind.y * step / -2.,
                ) * 4.
                    + u.camera_velocity;
                let model = Vec3::new(
                    corner.x * params[1][3],
                    corner.y * delta.y + params[1][3],
                    corner.z,
                );
                center
                    + u.inverse_xz_rotation.transform_vector3(model) * 0.5
                    + Vec3::new(corner.y * delta.x, 0., corner.y * delta.z)
            };
            let model_y = position.y / 2. - u.base_altitude;
            let opacity = ((0.5 - (fraction - 0.5).abs()) * 20.).min(1.)
                * (u.clip_level * (2.95 * 3.) - model_y).min(1.);
            let color = u.color.map(|c| c * opacity);
            if !position.is_finite() || color.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidInput("weather vertex overflow"));
            }
            mesh.vertices.push(Vertex {
                position,
                normal: Vec3::ZERO,
                uv,
                color,
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    Ok(mesh)
}
/// ParticleShader.MainPS/RainPS exact indoor/selected-floor discard predicate.
/// Call per fragment (not just at quad vertices) to preserve roof boundaries.
pub fn weather_fragment_visible(
    position: Vec3,
    u: WeatherUniforms,
    indoors: &[u8],
    width: u32,
    height: u32,
) -> Result<bool, Error> {
    if !position.is_finite()
        || width == 0
        || height == 0
        || indoors.len()
            != (width as usize)
                .checked_mul(height as usize)
                .ok_or(Error::InvalidInput("weather fragment map"))?
        || !u.blueprint_size.is_finite()
        || u.blueprint_size.x <= 0.
        || u.blueprint_size.y <= 0.
        || !u.base_altitude.is_finite()
        || !u.clip_level.is_finite()
        || !u.stories.is_finite()
    {
        return Err(Error::InvalidInput("weather fragment input"));
    }
    let model = position * 0.5 - Vec3::new(0., u.base_altitude, 0.);
    let level = model.y / (2.95 * 3.);
    let x = (model.x / u.blueprint_size.x * width as f32)
        .floor()
        .clamp(0., (width - 1) as f32) as u32;
    let y = (model.z / u.blueprint_size.y * height as f32)
        .floor()
        .clamp(0., (height - 1) as f32) as u32;
    let indoors_level = (f32::from(indoors[(y * width + x) as usize]) / 255. * u.stories).round();
    Ok(level < u.clip_level && indoors_level <= level)
}
