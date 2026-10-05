//! Positional visual attachment spaces. Admission observes A's relationships;
//! contact interpolation never creates, clears, or transfers gameplay ownership.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_render_core::{math::*, EntityRef};
#[derive(Clone, Copy, Debug)]
pub struct VisualSlot {
    pub offset: Vec3,
    pub height: u8,
}
#[derive(Clone, Copy, Debug)]
pub struct Attachment {
    pub child: EntityRef,
    pub container: EntityRef,
    pub slot: u16,
    pub endpoint_tile: Vec3,
}
pub fn avatar_slot_bone(slot: u16) -> Option<&'static str> {
    match slot {
        0 => Some("R_FINGER0"),
        1 => Some("HEAD"),
        2 => Some("PELVIS"),
        _ => None,
    }
}
fn rotate_xy(v: Vec3, direction: f32) -> Vec3 {
    let (s, c) = direction.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}
pub fn avatar_slot_position(
    bone: Vec3,
    position: Vec3,
    direction: f32,
    scale: f32,
) -> Result<Vec3> {
    if !bone.is_finite()
        || !position.is_finite()
        || !direction.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
    {
        return Err(AvatarError::Invalid("avatar slot input"));
    }
    let local = Vec3::new(bone.x, bone.z, bone.y) * (scale / 3.0);
    let out =
        rotate_xy(local, direction + std::f32::consts::PI) + position - Vec3::new(0.5, 0.5, 0.0);
    if !out.is_finite() {
        return Err(AvatarError::Invalid("avatar slot overflow"));
    }
    Ok(out)
}
pub fn object_slot_position(
    position: Vec3,
    direction: f32,
    slot: VisualSlot,
    avatar_scale: Option<f32>,
) -> Result<Vec3> {
    if !position.is_finite()
        || !direction.is_finite()
        || !slot.offset.is_finite()
        || slot.height > 9
    {
        return Err(AvatarError::Invalid("object slot input"));
    }
    let height = if slot.height == 0 { 5 } else { slot.height };
    let mut raw = slot.offset;
    if height != 5 {
        raw.z = [0.0, 2.5, 4.0, 4.0, 0.0, 0.0, 7.0, 4.0, 0.0][height as usize - 1];
    }
    let mut offset = Vec3::new(raw.x / 16.0, raw.y / 16.0, raw.z / 5.0);
    let mut center = Vec3::ZERO;
    if let Some(scale) = avatar_scale {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(AvatarError::Invalid("avatar occupancy scale"));
        }
        offset.z = 0.0;
        center = Vec3::new(0.5, 0.5, (1.0 - scale) / 2.0);
    }
    let out = rotate_xy(offset, direction) + position + center;
    if !out.is_finite() {
        return Err(AvatarError::Invalid("object slot overflow"));
    }
    Ok(out)
}
/// Draw transform: source Scale * RotationY(pi-direction) * EntityWorld,
/// transposed once into the core's column-vector convention.
pub fn avatar_world(position_tile: Vec3, direction: f32, scale: f32, slope: Quat) -> Result<Mat4> {
    if !position_tile.is_finite()
        || !direction.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
        || !unit(slope)
    {
        return Err(AvatarError::Invalid("avatar world"));
    }
    let graphics = Vec3::new(
        position_tile.x * 3.0,
        position_tile.z * 3.0,
        position_tile.y * 3.0,
    );
    let facing = Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI - direction)
        .ok_or(AvatarError::Invalid("facing"))?;
    let world = Mat4::from_translation(graphics)
        * Mat4::from_quat(slope)
        * Mat4::from_quat(facing)
        * Mat4::from_scale(Vec3::ONE * scale);
    if !crate::rig::matrix_finite(world) {
        return Err(AvatarError::Invalid("world overflow"));
    }
    Ok(world)
}
#[derive(Clone, Debug)]
pub struct AttachmentRegistry {
    attachments: Vec<Attachment>,
    max: usize,
}
impl AttachmentRegistry {
    pub fn new(max: usize) -> Self {
        Self {
            attachments: vec![],
            max,
        }
    }
    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }
    pub fn admit(&mut self, live: &[EntityRef], attachments: Vec<Attachment>) -> Result<()> {
        if live.len() > 65_535 || attachments.len() > self.max {
            return Err(AvatarError::Limit("attachments"));
        }
        let live_set: BTreeSet<_> = live.iter().copied().collect();
        let ids: BTreeSet<_> = live.iter().map(|e| e.object_id).collect();
        if live_set.len() != live.len()
            || ids.len() != live.len()
            || live.iter().any(|e| e.generation == 0)
        {
            return Err(AvatarError::Invalid("live identities"));
        }
        let mut links = BTreeMap::new();
        let mut slots = BTreeSet::new();
        for a in &attachments {
            if a.child == a.container
                || !live_set.contains(&a.child)
                || !live_set.contains(&a.container)
                || !a.endpoint_tile.is_finite()
                || links.insert(a.child, a.container).is_some()
                || !slots.insert((a.container, a.slot))
            {
                return Err(AvatarError::Invalid("admitted attachment"));
            }
        }
        for a in &attachments {
            let mut seen = BTreeSet::new();
            let mut node = a.child;
            while let Some(&parent) = links.get(&node) {
                if !seen.insert(node) {
                    return Err(AvatarError::Invalid("attachment cycle"));
                }
                node = parent;
            }
        }
        self.attachments = attachments;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.attachments.clear();
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ContactInterpolation {
    previous: Vec3,
    target: Vec3,
}
impl ContactInterpolation {
    pub fn new(point: Vec3) -> Self {
        Self {
            previous: point,
            target: point,
        }
    }
    pub fn sample(&self, next: Vec3, fraction: f32) -> Result<Vec3> {
        if !next.is_finite()
            || !self.previous.is_finite()
            || !fraction.is_finite()
            || !(0.0..=1.0).contains(&fraction)
        {
            return Err(AvatarError::Invalid("contact sample"));
        }
        let out = if (next - self.previous).length() > 1.5 {
            next
        } else {
            self.previous.lerp(next, fraction)
        };
        if !out.is_finite() {
            return Err(AvatarError::Invalid("contact overflow"));
        }
        Ok(out)
    }
    pub fn commit(&mut self, point: Vec3) -> Result<()> {
        if !point.is_finite() {
            return Err(AvatarError::Invalid("contact point"));
        }
        self.previous = if (point - self.target).length() > 1.5 {
            point
        } else {
            self.target
        };
        self.target = point;
        Ok(())
    }
    pub fn sample_relative(
        &self,
        container: Vec3,
        next_offset: Vec3,
        fraction: f32,
    ) -> Result<Vec3> {
        let out = container + self.sample(next_offset, fraction)?;
        if !out.is_finite() {
            return Err(AvatarError::Invalid("relative contact"));
        }
        Ok(out)
    }
    /// Called on an admitted route interruption, reset, or relationship change.
    pub fn interrupt(&mut self, point: Vec3) -> Result<()> {
        if !point.is_finite() {
            return Err(AvatarError::Invalid("contact interruption"));
        }
        *self = Self::new(point);
        Ok(())
    }
}
