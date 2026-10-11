//! Pure post-tick sampling. This module has no marker execution or gameplay completion API.
use crate::normalized::{quat, vec3};
use crate::*;
use std::sync::Arc;
use wonderland_render_core::{AssetKey, EntityRef};
#[derive(Clone, Debug)]
pub struct Clip {
    source: Animation,
    pub key: AssetKey,
    pub rig_key: AssetKey,
    motion_bones: Vec<Option<usize>>,
    resource: String,
}
#[derive(Clone, Debug)]
pub struct TimelineLayer {
    pub clip: Arc<Clip>,
    pub current_frame: f32,
    pub speed: f32,
    pub weight: f32,
    pub backwards: bool,
    pub end_reached: bool,
    pub looping: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    pub layers: Vec<TimelineLayer>,
    pub carry: Option<CarryPose>,
}
#[derive(Clone, Debug)]
pub struct CarryPose {
    pub clip: Arc<Clip>,
    pub frame: f32,
}
impl Clip {
    pub fn new(rig: &Rig, source: Animation, key: AssetKey, limits: AvatarLimits) -> Result<Self> {
        let resource = source.name.clone();
        Self::new_resolved(rig, source, key, resource, limits)
    }
    pub fn new_resolved(
        rig: &Rig,
        source: Animation,
        key: AssetKey,
        resource: String,
        limits: AvatarLimits,
    ) -> Result<Self> {
        if resource.is_empty() || resource.len() > 1024 || !resource.is_ascii() {
            return Err(AvatarError::Invalid("resolved animation resource"));
        }
        if source.coordinate_policy != CoordinatePolicy::FreeSo
            || source.version != 2
            || source.name.is_empty()
            || source.name.len() > 1024
            || !source.name.is_ascii()
            || !source.duration_ms.get().is_finite()
            || !source.distance.get().is_finite()
        {
            return Err(AvatarError::Invalid("normalized clip"));
        }
        if source.motions.len() > limits.max_motions
            || source.translations.len() > limits.max_samples
            || source.rotations.len() > limits.max_samples
            || source.num_frames > 1_000_000
        {
            return Err(AvatarError::Limit("animation"));
        }
        if source.translations.iter().any(|v| !vec3(*v).is_finite())
            || source.rotations.iter().any(|q| !unit(quat(*q)))
        {
            return Err(AvatarError::Invalid("nonfinite/nonunit channel"));
        }
        for m in &source.motions {
            if !m.duration_ms.get().is_finite()
                || m.frame_count > 1_000_000
                || m.bone_name.len() > 1024
            {
                return Err(AvatarError::Invalid("motion metadata"));
            }
            if (m.translation_flag == 1 || m.rotation_flag == 1) && m.frame_count == 0 {
                return Err(AvatarError::Invalid("active empty motion"));
            }
            if m.translation_flag == 1 {
                crate::mesh::range(
                    m.first_translation_index,
                    m.frame_count as i32,
                    source.translations.len(),
                )?;
            }
            if m.rotation_flag == 1 {
                crate::mesh::range(
                    m.first_rotation_index,
                    m.frame_count as i32,
                    source.rotations.len(),
                )?;
            }
        }
        if bincode::serialized_size(&source)
            .map_err(|_| AvatarError::Invalid("animation metadata"))?
            > limits.max_metadata_bytes as u64
        {
            return Err(AvatarError::Limit("animation metadata"));
        }
        let motion_bones = source
            .motions
            .iter()
            .map(|m| rig.bone_index(&m.bone_name))
            .collect();
        Ok(Self {
            source,
            key,
            rig_key: rig.key,
            motion_bones,
            resource,
        })
    }
    pub fn source(&self) -> &Animation {
        &self.source
    }
    /// Validate A's resolved resource string and frame count plus the effective B digest.
    pub fn matches_projection(
        &self,
        resource: &str,
        num_frames: u32,
        source_digest: AssetKey,
    ) -> bool {
        self.resource.eq_ignore_ascii_case(resource)
            && self.source.num_frames == num_frames
            && self.key == source_digest
    }
    fn apply(&self, pose: &mut Pose, frame: f32, weight: f32) -> Result<()> {
        if self.rig_key != pose.rig_key
            || !frame.is_finite()
            || frame.abs() > 16_000_000.0
            || !weight.is_finite()
        {
            return Err(AvatarError::Invalid("clip pose/frame/weight"));
        }
        let integer = (frame as i32).clamp(0, self.source.num_frames as i32) as usize;
        let fraction = frame % 1.0;
        for (motion, bone) in self.source.motions.iter().zip(&self.motion_bones) {
            let bone = match bone {
                Some(b) => *b,
                None => continue,
            };
            if motion.frame_count == 0 {
                continue;
            }
            let i = integer.min(motion.frame_count as usize - 1);
            let j = if integer + 1 >= motion.frame_count as usize {
                i
            } else {
                i + 1
            };
            let local = &mut pose.locals[bone];
            if motion.translation_flag == 1 {
                let offset = motion.first_translation_index as usize;
                let a = vec3(self.source.translations[offset + i]);
                let value = if fraction >= 0.0 {
                    a.lerp(vec3(self.source.translations[offset + j]), fraction)
                } else {
                    a
                };
                local.translation = if weight == 1.0 {
                    value
                } else {
                    local.translation.lerp(value, weight)
                };
            }
            if motion.rotation_flag == 1 {
                let offset = motion.first_rotation_index as usize;
                let a = quat(self.source.rotations[offset + i]);
                let value = if fraction >= 0.0 {
                    a.slerp(quat(self.source.rotations[offset + j]), fraction)
                } else {
                    a
                };
                local.rotation = if weight == 1.0 {
                    value
                } else {
                    local.rotation.slerp(value, weight)
                };
            }
        }
        Ok(())
    }
}
pub fn sample_timeline(
    rig: &Rig,
    pose: &mut Pose,
    timeline: &Timeline,
    fraction: f32,
) -> Result<()> {
    if !fraction.is_finite() || !(0.0..1.0).contains(&fraction) || timeline.layers.len() > 64 {
        return Err(AvatarError::Invalid("presentation fraction/layer budget"));
    }
    let mut candidate = pose.clone();
    candidate.rebuild(rig)?;
    let mut prefix = 0.0f32;
    for layer in &timeline.layers {
        if !layer.weight.is_finite()
            || !layer.current_frame.is_finite()
            || layer.current_frame.abs() > 16_000_000.0
            || !layer.speed.is_finite()
            || layer.speed.abs() > 1_000_000.0
            || layer.clip.rig_key != rig.key
        {
            return Err(AvatarError::Invalid("timeline projection"));
        }
        prefix += layer.weight;
        if !prefix.is_finite() {
            return Err(AvatarError::Invalid("prefix overflow"));
        }
        // Declared safety deviation: a zero prefix contributes no channel writes.
        // Source performs 0/0; preserving retained channels avoids permanent NaN poses.
        if !layer.end_reached && prefix != 0.0 {
            let frame = layer.current_frame
                + if layer.backwards {
                    -layer.speed * fraction
                } else {
                    layer.speed * fraction
                };
            layer
                .clip
                .apply(&mut candidate, frame, layer.weight / prefix)?;
        }
    }
    if let Some(carry) = &timeline.carry {
        carry.clip.apply(&mut candidate, carry.frame.trunc(), 1.0)?;
    }
    candidate.rebuild(rig)?;
    *pose = candidate;
    Ok(())
}
/// Retention is committed once per admitted simulation tick. Every draw samples
/// the same pre-commit baseline, so refresh cadence cannot compound layer blends.
#[derive(Clone, Debug)]
pub struct PosePlayer {
    pub entity: EntityRef,
    retained: Pose,
    baseline: Pose,
    timeline: Timeline,
    tick: Option<u64>,
}
impl PosePlayer {
    pub fn new(entity: EntityRef, rig: &Rig) -> Self {
        let retained = rig.bind_pose();
        Self {
            entity,
            baseline: retained.clone(),
            retained,
            timeline: Timeline::default(),
            tick: None,
        }
    }
    pub fn retained(&self) -> &Pose {
        &self.retained
    }
    pub fn commit(&mut self, tick: u64, rig: &Rig, timeline: Timeline) -> Result<bool> {
        if let Some(previous) = self.tick {
            if tick == previous {
                return Ok(false);
            }
            if tick < previous {
                return Err(AvatarError::Stale);
            }
        }
        let baseline = self.retained.clone();
        let mut retained = baseline.clone();
        sample_timeline(rig, &mut retained, &timeline, 0.0)?;
        self.baseline = baseline;
        self.retained = retained;
        self.timeline = timeline;
        self.tick = Some(tick);
        Ok(true)
    }
    pub fn sample(&self, rig: &Rig, fraction: f32) -> Result<Pose> {
        let mut p = self.baseline.clone();
        sample_timeline(rig, &mut p, &self.timeline, fraction)?;
        Ok(p)
    }
    /// Explicit resynchronization boundary: A does not snapshot the legacy visual skeleton.
    pub fn reset(&mut self, entity: EntityRef, rig: &Rig) {
        *self = Self::new(entity, rig);
    }
}
