use crate::*;
use wonderland_render_core::{EntityRef, Mat4, Transform, Vec3, ViewMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraIntent {
    pub center_tile: Vec3,
    pub zoom: Zoom,
    pub precise_zoom: f32,
    pub rotation: Rotation,
    pub selected_level: i16,
    pub selected: Option<EntityRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    Sprites,
    Geometry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewPolicy {
    pub objects: Representation,
    pub architecture: Representation,
    pub immediate: bool,
    pub safe_2d: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSample {
    pub pose: Transform,
    pub projection: Mat4,
    pub remaining_weight: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    pub mode: ViewMode,
    pub intent: CameraIntent,
    pose: Transform,
    projection: Mat4,
    rotation_offset: f32,
    transition: Option<Transition>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Transition {
    from_pose: Transform,
    from_projection: Mat4,
    duration: f32,
    remaining: f32,
}
impl ViewState {
    pub fn new(
        mode: ViewMode,
        intent: CameraIntent,
        pose: Transform,
        projection: Mat4,
    ) -> Result<Self> {
        validate_camera(intent, pose, projection)?;
        Ok(Self {
            mode,
            intent,
            pose,
            projection,
            rotation_offset: 0.,
            transition: None,
        })
    }
    /// An interrupted transition starts from the currently sampled camera. The
    /// immutable camera/selection intent survives every representation change.
    pub fn switch_to(
        &mut self,
        mode: ViewMode,
        pose: Transform,
        projection: Mat4,
        duration: f32,
    ) -> Result<()> {
        validate_camera(self.intent, pose, projection)?;
        if !duration.is_finite() || duration < 0. {
            return Err(IsoError::Invalid("transition duration"));
        }
        let sample = self.sample_camera();
        self.mode = mode;
        self.pose = pose;
        self.projection = projection;
        self.transition = if duration == 0. {
            None
        } else {
            Some(Transition {
                from_pose: sample.pose,
                from_projection: sample.projection,
                duration,
                remaining: 1.,
            })
        };
        Ok(())
    }
    pub fn advance(&mut self, elapsed_seconds: f32) -> Result<()> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0. {
            return Err(IsoError::Invalid("presentation elapsed time"));
        }
        if let Some(t) = self.transition.as_mut() {
            t.remaining = (t.remaining - elapsed_seconds / t.duration).max(0.);
            if t.remaining == 0. {
                self.transition = None;
            }
        }
        Ok(())
    }
    pub fn sample_camera(&self) -> CameraSample {
        match self.transition {
            None => CameraSample {
                pose: self.pose,
                projection: self.projection,
                remaining_weight: 0.,
            },
            Some(t) => {
                let pose = t
                    .from_pose
                    .interpolate(self.pose, 1. - t.remaining)
                    .expect("validated transition endpoints");
                let power = if self.mode == ViewMode::Full3D {
                    1. / 50.
                } else {
                    5.
                };
                let fraction = t.remaining.powf(power);
                let mut projection = self.projection;
                for c in 0..4 {
                    for r in 0..4 {
                        projection.cols[c][r] = self.projection.cols[c][r] * (1. - fraction)
                            + t.from_projection.cols[c][r] * fraction;
                    }
                }
                CameraSample {
                    pose,
                    projection,
                    remaining_weight: t.remaining,
                }
            }
        }
    }
    pub fn policy(&self) -> ViewPolicy {
        let safe = self.mode != ViewMode::Full3D
            && self.transition.is_none()
            && self.rotation_offset == 0.;
        let objects = if safe {
            Representation::Sprites
        } else {
            Representation::Geometry
        };
        let architecture = if safe && self.mode == ViewMode::Full2D {
            Representation::Sprites
        } else {
            Representation::Geometry
        };
        ViewPolicy {
            objects,
            architecture,
            immediate: !safe,
            safe_2d: safe,
        }
    }
    pub fn sprite_zoom(&self) -> Zoom {
        if self.mode == ViewMode::Full3D {
            Zoom::Near
        } else {
            self.intent.zoom
        }
    }
    pub fn set_rotation_offset(&mut self, degrees: f32) -> Result<()> {
        if !degrees.is_finite() {
            return Err(IsoError::Invalid("rotation offset"));
        }
        self.rotation_offset = degrees;
        Ok(())
    }
}
fn validate_camera(intent: CameraIntent, pose: Transform, projection: Mat4) -> Result<()> {
    if !intent.center_tile.is_finite()
        || !intent.precise_zoom.is_finite()
        || intent.precise_zoom <= 0.
        || intent.selected_level < 1
        || intent
            .selected
            .map_or(false, |r| r.generation == 0 || r.object_id == 0)
        || !pose.is_valid()
        || projection.cols.iter().flatten().any(|x| !x.is_finite())
    {
        return Err(IsoError::Invalid("camera intent or sample"));
    }
    Ok(())
}
/// Source yaw quantization includes a quarter-pi bias and nearest-even ties.
pub fn nearest_2d_rotation(yaw_radians: f32) -> Result<Rotation> {
    if !yaw_radians.is_finite() {
        return Err(IsoError::Invalid("camera yaw"));
    }
    // DirectionUtils.PosMod and Math.PI promote the source f32 yaw to double.
    // Keep that precision through Math.Round's nearest-even boundary decision.
    let yaw = f64::from(yaw_radians);
    let tau = std::f64::consts::TAU;
    let wrapped = (yaw % tau + tau) % tau;
    let quarter_turns = (wrapped / std::f64::consts::PI + 0.25) * 2.;
    let lower = quarter_turns.floor();
    let fraction = quarter_turns - lower;
    let rounded = if fraction > 0.5 || (fraction == 0.5 && lower % 2.0 != 0.0) {
        lower + 1.
    } else {
        lower
    };
    Ok(Rotation::ALL[rounded as usize % 4])
}
pub fn rotation_offset(from_degrees: f32, elapsed_seconds: f32) -> Result<f32> {
    if !from_degrees.is_finite() || !elapsed_seconds.is_finite() || elapsed_seconds < 0. {
        return Err(IsoError::Invalid("rotation easing"));
    }
    let from = (from_degrees + 180.).rem_euclid(360.) - 180.;
    let p = (elapsed_seconds * 3.).min(1.);
    Ok(from * ((p * std::f32::consts::PI).cos() + 1.) * 0.5)
}
pub fn smooth_zoom_scale(previous_scale: f32, elapsed_seconds: f32) -> Result<f32> {
    if !previous_scale.is_finite()
        || previous_scale <= 0.
        || !elapsed_seconds.is_finite()
        || elapsed_seconds < 0.
    {
        return Err(IsoError::Invalid("zoom easing"));
    }
    let timer = (elapsed_seconds * 60.).min(15.);
    let blend = (timer * std::f32::consts::PI / 30.).sin();
    Ok(previous_scale * (1. - blend) + blend)
}
