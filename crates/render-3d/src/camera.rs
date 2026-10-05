//! Presentation camera control; terrain/floor height is not wall collision.
use crate::Error;
use wonderland_render_core::{EntityRef, Mat4, Transform, Vec2, Vec3};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPose {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub hide_head: Option<EntityRef>,
}
impl CameraPose {
    pub fn view_projection(self, aspect: f32) -> Result<Mat4, Error> {
        let view = Mat4::look_at_rh(self.position, self.target, self.up)
            .ok_or(Error::InvalidInput("camera view"))?;
        let projection = Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far)
            .ok_or(Error::InvalidInput("camera projection"))?;
        let result = projection * view;
        if !result.is_finite() {
            return Err(Error::InvalidInput("camera view-projection overflow"));
        }
        Ok(result)
    }
}

/// City controls follow CityCamera3D, in city coordinates (one map pixel = one
/// world unit). OrbitCamera below is the separate lot-space camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CityCameraMode {
    Orbit,
    FirstPerson { height: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityCamera {
    pub yaw: f32,
    pub pitch_control: f32,
    pub zoom: f32,
    pub target_zoom: f32,
    pub center: Vec2,
    pub cam_height: f32,
    pub mode: CityCameraMode,
}
impl Default for CityCamera {
    fn default() -> Self {
        Self {
            yaw: -3. * std::f32::consts::FRAC_PI_4,
            pitch_control: 0.,
            zoom: 3.7,
            target_zoom: 0.25,
            center: Vec2::new(184., 328.),
            cam_height: 0.,
            mode: CityCameraMode::Orbit,
        }
    }
}
impl CityCamera {
    pub fn pose(self) -> Result<CameraPose, Error> {
        if !self.yaw.is_finite()
            || !self.pitch_control.is_finite()
            || !self.zoom.is_finite()
            || !self.target_zoom.is_finite()
            || !self.center.is_finite()
            || !self.cam_height.is_finite()
        {
            return Err(Error::InvalidInput("city camera"));
        }
        let pitch = self.pitch_control.clamp(0., std::f32::consts::PI);
        let base = Vec3::new(self.center.x, self.cam_height + 0.5, self.center.y);
        let (position, target) = match self.mode {
            CityCameraMode::Orbit => {
                let angle = (1. - pitch.cos()) * std::f32::consts::PI * 0.245;
                let zoom = self.zoom.clamp(0., 100.);
                let z = zoom * zoom;
                let base_distance = if self.target_zoom > 2. {
                    3.5 - (self.target_zoom - 2.) * 2.
                } else {
                    3.5
                };
                let near = rotate_z(Vec3::new(base_distance, 0., 0.), angle);
                let far = rotate_z(Vec3::new(1.30 * z, z, 0.), angle / 2.);
                (base + rotate_y(near + far, self.yaw), base)
            }
            CityCameraMode::FirstPerson { height } => {
                if !height.is_finite() {
                    return Err(Error::InvalidInput("city first-person height"));
                }
                let position = base + Vec3::Y * height;
                let angle = (pitch - std::f32::consts::FRAC_PI_2) * 0.99;
                let forward = rotate_y(rotate_z(Vec3::new(-10., 0., 0.), angle), self.yaw);
                (position, position + forward)
            }
        };
        if !position.is_finite() || !target.is_finite() {
            return Err(Error::InvalidInput("city camera overflow"));
        }
        Ok(CameraPose {
            position,
            target,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_4,
            near: 0.25,
            far: 800.,
            hide_head: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch_control: f32,
    pub zoom: f32,
    pub center: Vec2,
    pub cam_height: f32,
}
impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch_control: 1.,
            zoom: 3.7,
            center: Vec2::ZERO,
            cam_height: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoZoom {
    Near,
    Medium,
    Far,
}
impl OrbitCamera {
    pub fn pose(self) -> Result<CameraPose, Error> {
        if !self.yaw.is_finite()
            || !self.pitch_control.is_finite()
            || !self.zoom.is_finite()
            || !self.center.is_finite()
            || !self.cam_height.is_finite()
        {
            return Err(Error::InvalidInput("orbit camera"));
        }
        let pitch = self.pitch_control.clamp(0., std::f32::consts::PI);
        let zoom = self.zoom.clamp(0., 100.);
        let angle = (1. - pitch.cos()) * std::f32::consts::PI * 0.245;
        let z = zoom * zoom;
        let near = rotate_z(Vec3::new(10., 0., 0.), angle);
        let far = rotate_z(Vec3::new(1.30 * z, z, 0.), angle / 2.);
        let relative = rotate_y(near + far, self.yaw);
        let target = Vec3::new(self.center.x * 3., self.cam_height + 3., self.center.y * 3.);
        if !target.is_finite() || !(target + relative).is_finite() {
            return Err(Error::InvalidInput("orbit camera overflow"));
        }
        Ok(CameraPose {
            position: target + relative,
            target,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_4,
            near: 1.,
            far: 800.,
            hide_head: None,
        })
    }
    pub fn inherit_2d(&mut self, rotation: u8, zoom: IsoZoom) -> Result<(), Error> {
        if rotation > 3 {
            return Err(Error::InvalidInput("2D rotation"));
        }
        self.yaw = std::f32::consts::PI * (f32::from(rotation) / 2. - 0.25);
        self.pitch_control = 0.;
        self.zoom = match zoom {
            IsoZoom::Near => 3.7,
            IsoZoom::Medium => 7.,
            IsoZoom::Far => 11.,
        };
        Ok(())
    }
}
fn rotate_z(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}
fn rotate_y(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}
pub fn cut_rotation(yaw: f32) -> Result<u8, Error> {
    if !yaw.is_finite() {
        return Err(Error::InvalidInput("cut rotation"));
    }
    let value = f64::from(yaw / std::f32::consts::FRAC_PI_2 + 0.5);
    let floor = value.floor();
    let fraction = value - floor;
    let rounded = if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.
    } else if floor.rem_euclid(2.) == 0. {
        floor
    } else {
        floor + 1.
    };
    Ok(rounded.rem_euclid(4.) as u8)
}
pub fn damp_height(current: f32, target: f32, seconds: f32) -> Result<f32, Error> {
    if !current.is_finite() || !target.is_finite() || !seconds.is_finite() || seconds < 0. {
        return Err(Error::InvalidInput("camera height/time"));
    }
    let retention = 0.8f64.powf(60. * f64::from(seconds));
    Ok((f64::from(current) * retention + f64::from(target) * (1. - retention)) as f32)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstPersonCamera {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch_control: f32,
    pub fov_y: f32,
    pub captured: bool,
    pub focused: bool,
}
impl FirstPersonCamera {
    pub fn pose(self) -> Result<CameraPose, Error> {
        if !self.position.is_finite()
            || !self.velocity.is_finite()
            || !self.yaw.is_finite()
            || !self.pitch_control.is_finite()
            || !self.fov_y.is_finite()
            || self.fov_y <= 0.
            || self.fov_y >= std::f32::consts::PI
        {
            return Err(Error::InvalidInput("first person camera"));
        }
        let angle = (self.pitch_control.clamp(0., std::f32::consts::PI)
            - std::f32::consts::FRAC_PI_2)
            * 0.99;
        let forward = rotate_y(rotate_z(Vec3::new(-10., 0., 0.), angle), self.yaw);
        Ok(CameraPose {
            position: self.position,
            target: self.position + forward,
            up: Vec3::Y,
            fov_y: self.fov_y,
            near: 1.,
            far: 800.,
            hide_head: None,
        })
    }
    /// Exact exponential integration at explicit presentation time. Acceleration is
    /// in graphics units per second squared; this never emits movement commands.
    pub fn advance(
        &mut self,
        acceleration: Vec3,
        seconds: f32,
        floor_height: Option<f32>,
    ) -> Result<(), Error> {
        self.pose()?;
        if !acceleration.is_finite()
            || !seconds.is_finite()
            || seconds < 0.
            || floor_height.map(|h| !h.is_finite()).unwrap_or(false)
        {
            return Err(Error::InvalidInput("flight input"));
        }
        let mut next = *self;
        if self.captured {
            let a = if self.focused {
                acceleration
            } else {
                Vec3::ZERO
            };
            let lambda = -60. * 0.9f64.ln();
            let dt = f64::from(seconds);
            let decay = (-lambda * dt).exp();
            let v_weight = -(-lambda * dt).exp_m1() / lambda;
            let a_weight = (dt - v_weight) / lambda;
            next.position =
                self.position + self.velocity * (v_weight as f32) + a * (a_weight as f32);
            next.velocity = self.velocity * (decay as f32) + a * (v_weight as f32);
        }
        if let Some(floor) = floor_height {
            next.position.y = next.position.y.max(floor + 1.);
            if next.position.y == floor + 1. && next.velocity.y < 0. {
                next.velocity.y = 0.;
            }
        }
        next.pose()?;
        *self = next;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadAnchor {
    pub entity: EntityRef,
    pub position_tile: Vec3,
    pub scale: f32,
}
pub fn direct_pose(anchor: HeadAnchor, yaw: f32, pitch: f32) -> Result<CameraPose, Error> {
    if anchor.entity.generation == 0
        || !anchor.position_tile.is_finite()
        || !anchor.scale.is_finite()
        || anchor.scale <= 0.
    {
        return Err(Error::InvalidInput("direct head anchor"));
    }
    let mut pose = FirstPersonCamera {
        position: Vec3::new(
            anchor.position_tile.x * 3.,
            anchor.position_tile.z * 3. + 0.25 * anchor.scale,
            anchor.position_tile.y * 3.,
        ),
        velocity: Vec3::ZERO,
        yaw,
        pitch_control: pitch,
        fov_y: 0.9,
        captured: false,
        focused: false,
    }
    .pose()?;
    pose.near = 0.5;
    pose.hide_head = Some(anchor.entity);
    Ok(pose)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraCollisionPolicy {
    TerrainAndCurrentFloorHeightOnly,
    ExternalResolvedPose,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSample {
    pub transform: Transform,
    pub projection: Mat4,
    pub complete: bool,
}
pub fn sample_transition(
    from: Transform,
    to: Transform,
    from_projection: Mat4,
    to_projection: Mat4,
    elapsed: f32,
    duration: f32,
) -> Result<CameraSample, Error> {
    if !elapsed.is_finite()
        || !duration.is_finite()
        || elapsed < 0.
        || duration < 0.
        || from_projection
            .cols
            .iter()
            .flatten()
            .chain(to_projection.cols.iter().flatten())
            .any(|v| !v.is_finite())
    {
        return Err(Error::InvalidInput("camera transition"));
    }
    let f = if duration == 0. {
        1.
    } else {
        (elapsed / duration).clamp(0., 1.)
    };
    let transform = from
        .interpolate(to, f)
        .ok_or(Error::InvalidInput("camera transform"))?;
    let mut projection = from_projection;
    for c in 0..4 {
        for r in 0..4 {
            projection.cols[c][r] =
                from_projection.cols[c][r] * (1. - f) + to_projection.cols[c][r] * f;
        }
    }
    if !projection.is_finite() {
        return Err(Error::InvalidInput("camera transition overflow"));
    }
    Ok(CameraSample {
        transform,
        projection,
        complete: f == 1.,
    })
}
pub const DEFAULT_TRANSITION_SECONDS: f32 = 0.66;
