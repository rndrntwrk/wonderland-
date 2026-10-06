use crate::*;
use wonderland_render_core::math::*;
pub fn head_seek(neck: Mat4, target: Vec3, direction: f32) -> Result<Quat> {
    if !target.is_finite() || !direction.is_finite() || !crate::rig::matrix_finite(neck) {
        return Err(AvatarError::Invalid("look input"));
    }
    let facing = Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI - direction)
        .ok_or(AvatarError::Invalid("look facing"))?;
    let inverse = (Mat4::from_quat(facing) * neck)
        .inverse()
        .ok_or(AvatarError::Invalid("singular neck"))?;
    let diff = inverse.transform_point3(target);
    let horizontal = (-diff.y)
        .atan2(diff.z)
        .clamp(-65.0f32.to_radians(), 65.0f32.to_radians());
    let distance = ((diff.z * diff.z + diff.y * diff.y) as f64).sqrt();
    let vertical = ((diff.x as f64 / distance).atan() as f32)
        .clamp(-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4);
    let x = Quat::from_axis_angle(Vec3::X, horizontal)
        .ok_or(AvatarError::Invalid("look horizontal"))?;
    let y = Quat::from_axis_angle(Vec3::Y, vertical)
        .ok_or(AvatarError::Invalid("look vertical/zero target"))?;
    Ok(x * y)
}
pub fn smooth_head(current: Quat, target: Quat, elapsed_frames: f32) -> Result<Quat> {
    if !unit(current) || !unit(target) || !elapsed_frames.is_finite() || elapsed_frames < 0.0 {
        return Err(AvatarError::Invalid("head smoothing"));
    }
    let inner =
        current.x * target.x + current.y * target.y + current.z * target.z + current.w * target.w;
    let distance = (2.0 * (inner * inner) - 1.0).acos();
    if current == target || distance == 0.0 || distance.is_nan() {
        return Ok(current);
    }
    Ok(current.slerp(target, ((0.2 * elapsed_frames) / distance).min(1.0)))
}
pub fn apply_head_seek(rig: &Rig, pose: &mut Pose, target: Quat, weight: f32) -> Result<()> {
    if !unit(target) || !weight.is_finite() {
        return Err(AvatarError::Invalid("head seek pose"));
    }
    let head = rig
        .bone_index("HEAD")
        .ok_or_else(|| AvatarError::MissingBone("HEAD".into()))?;
    let mut candidate = pose.clone();
    candidate.rebuild(rig)?;
    let current = candidate.locals[head].rotation;
    candidate.locals[head].rotation = if weight == 1.0 {
        target
    } else {
        current.slerp(target, weight)
    };
    candidate.rebuild(rig)?;
    *pose = candidate;
    Ok(())
}
/// A owns the target, timeout, state and weight. Only this draw's fractional fade is sampled.
pub fn seek_weight(committed_frames: f32, fraction: f32, fading_out: bool) -> Result<f32> {
    if !committed_frames.is_finite() || !fraction.is_finite() || !(0.0..1.0).contains(&fraction) {
        return Err(AvatarError::Invalid("seek fade"));
    }
    Ok(((committed_frames + if fading_out { -fraction } else { fraction }) / 15.0).min(1.0))
}
