//! Source free-flight camera, entirely outside authoritative Sim movement.
use crate::{ViewportControls, WorldDocument, WorldError, orbit_camera};
use serde::{Deserialize, Serialize};
use wonderland_render_3d::camera::{CameraPose, FirstPersonCamera};
use wonderland_render_core::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreeCameraState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub fov_y: f32,
    pub saved_pitch: f32,
}
fn vector(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}
fn array(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}
fn camera(state: FreeCameraState, controls: ViewportControls) -> FirstPersonCamera {
    FirstPersonCamera {
        position: vector(state.position),
        velocity: vector(state.velocity),
        yaw: controls.yaw_radians,
        pitch_control: controls.pitch_radians,
        fov_y: state.fov_y,
        captured: true,
        focused: true,
    }
}
fn floor_height(
    world: &WorldDocument,
    controls: ViewportControls,
    p: Vec3,
) -> Result<f32, WorldError> {
    let width = usize::from(world.lot.width);
    let height = usize::from(world.lot.height);
    let x = (p.x / 3.).clamp(0., width as f32);
    let y = (p.z / 3.).clamp(0., height as f32);
    let bx = (x.floor() as usize).min(width - 1);
    let by = (y.floor() as usize).min(height - 1);
    let u = x - bx as f32;
    let v = y - by as f32;
    let raw = |x: usize, y: usize| {
        world
            .lot
            .terrain
            .corners
            .get(y * (width + 1) + x)
            .copied()
            .map(f32::from)
            .ok_or_else(|| WorldError("missing camera terrain corner".into()))
    };
    // Same corner interpolation and graphics-unit conversion as rendered terrain.
    let altitude = (raw(bx, by)? * (1. - u) + raw(bx + 1, by)? * u) * (1. - v)
        + (raw(bx, by + 1)? * (1. - u) + raw(bx + 1, by + 1)? * u) * v;
    Ok(
        (altitude - f32::from(world.lot.terrain.base_alt)) * 9. / 160.
            + f32::from(controls.visible_level - 1) * 8.85,
    )
}

pub fn active_camera(
    world: &WorldDocument,
    controls: ViewportControls,
    aspect: f32,
) -> Result<CameraPose, WorldError> {
    let orbit = orbit_camera(world, controls, aspect)?;
    match controls.free_camera {
        None => orbit.pose().map_err(Into::into),
        Some(state) => {
            if !state.saved_pitch.is_finite() {
                return Err(WorldError("invalid saved camera pitch".into()));
            }
            let mut camera = camera(state, controls);
            camera.advance(
                Vec3::ZERO,
                0.,
                Some(floor_height(world, controls, camera.position)?),
            )?;
            camera.pose().map_err(Into::into)
        }
    }
}
/// Enter at the orbit's actual eye and direction. Returning restores the orbit
/// framing; free-flight never changes the Sim, lot, or construction state.
pub fn toggle_free_camera(
    world: &WorldDocument,
    controls: &mut ViewportControls,
    aspect: f32,
) -> Result<(), WorldError> {
    if let Some(state) = controls.free_camera {
        controls.pitch_radians = state.saved_pitch;
        controls.free_camera = None;
        return Ok(());
    }
    let pose = active_camera(world, *controls, aspect)?;
    let direction = pose.target - pose.position;
    let length =
        (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
    let saved_pitch = controls.pitch_radians;
    controls.yaw_radians = direction.z.atan2(-direction.x);
    controls.pitch_radians = std::f32::consts::FRAC_PI_2 - (direction.y / length).asin() / 0.99;
    controls.free_camera = Some(FreeCameraState {
        position: array(pose.position),
        velocity: [0.; 3],
        fov_y: pose.fov_y,
        saved_pitch,
    });
    Ok(())
}
/// Local input is right/forward/up, normalized before source damping. Suspended
/// presentation time is bounded; the caller clears input on focus/mode loss.
pub fn advance_free_camera(
    world: &WorldDocument,
    controls: &mut ViewportControls,
    input: [f32; 3],
    seconds: f32,
    fast: bool,
) -> Result<(), WorldError> {
    let Some(state) = controls.free_camera else {
        return Ok(());
    };
    crate::scene::validate_controls(world, *controls)?;
    crate::document::checked_area(world.lot.width, world.lot.height)?;
    if input.iter().any(|v| !v.is_finite()) || !seconds.is_finite() || seconds < 0. {
        return Err(WorldError("invalid flight input".into()));
    }
    let mut camera = camera(state, *controls);
    let (sin, cos) = controls.yaw_radians.sin_cos();
    let length = input.iter().map(|v| v * v).sum::<f32>().sqrt().max(1.);
    let speed = if fast { 180. } else { 60. } / length;
    let acceleration = Vec3::new(
        -input[0] * sin - input[1] * cos,
        input[2],
        -input[0] * cos + input[1] * sin,
    ) * speed;
    camera.advance(acceleration, seconds.min(0.05), None)?;
    camera.advance(
        Vec3::ZERO,
        0.,
        Some(floor_height(world, *controls, camera.position)?),
    )?;
    // Avoid endless repaint from imperceptible damping tails.
    if input == [0.; 3] && camera.velocity.length() < 0.001 {
        camera.velocity = Vec3::ZERO;
    }
    controls.free_camera = Some(FreeCameraState {
        position: array(camera.position),
        velocity: array(camera.velocity),
        ..state
    });
    Ok(())
}
pub fn stop_free_camera(controls: &mut ViewportControls) {
    if let Some(state) = &mut controls.free_camera {
        state.velocity = [0.; 3];
    }
}
