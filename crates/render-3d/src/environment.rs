//! Injected UTC environment and explicit unmeasured quality/LOD policies.
use crate::legacy_random::LegacyRandom;
use crate::Error;
use wonderland_render_core::ViewMode;
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
