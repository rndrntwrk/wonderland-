//! Presentation culling and pose cadence only. A's simulation always continues.
use crate::*;
use std::collections::BTreeSet;
use wonderland_render_core::{math::*, EntityRef};
#[derive(Clone, Copy, Debug)]
pub struct AvatarCandidate {
    pub reference: EntityRef,
    pub bounds: Aabb,
    pub visible: bool,
    pub level: i16,
    pub requires_endpoint: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct LodPolicy {
    pub near_distance: f32,
    pub far_distance: f32,
    pub medium_stride: u64,
    pub far_stride: u64,
    pub max_pose_updates: usize,
}
impl Default for LodPolicy {
    fn default() -> Self {
        Self {
            near_distance: 12.0,
            far_distance: 30.0,
            medium_stride: 2,
            far_stride: 4,
            max_pose_updates: 64,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodDecision {
    pub reference: EntityRef,
    pub draw: bool,
    pub update_pose: bool,
    pub stride: u64,
}
pub fn plan_lod(
    tick: u64,
    actors: &[AvatarCandidate],
    camera: Vec3,
    view: Aabb,
    level: i16,
    policy: LodPolicy,
) -> Result<Vec<LodDecision>> {
    if actors.len() > 65_535
        || !camera.is_finite()
        || Aabb::new(view.min, view.max).is_none()
        || !policy.near_distance.is_finite()
        || !policy.far_distance.is_finite()
        || policy.near_distance < 0.0
        || policy.far_distance < policy.near_distance
        || policy.medium_stride == 0
        || policy.far_stride == 0
    {
        return Err(AvatarError::Invalid("LOD inputs"));
    }
    let mut ids = BTreeSet::new();
    let mut references = BTreeSet::new();
    let mut rows = Vec::with_capacity(actors.len());
    let endpoint_count = actors.iter().filter(|a| a.requires_endpoint).count();
    if endpoint_count > policy.max_pose_updates {
        return Err(AvatarError::Limit("required endpoint updates"));
    }
    for actor in actors {
        if actor.reference.generation == 0
            || !ids.insert(actor.reference.object_id)
            || !references.insert(actor.reference)
            || Aabb::new(actor.bounds.min, actor.bounds.max).is_none()
        {
            return Err(AvatarError::Invalid("LOD identity/bounds"));
        }
        let center = actor.bounds.min * 0.5 + actor.bounds.max * 0.5;
        let distance = (center - camera).length();
        if !distance.is_finite() {
            return Err(AvatarError::Invalid("LOD distance"));
        }
        let draw = actor.visible && actor.level <= level && actor.bounds.intersects(view);
        let stride = if distance <= policy.near_distance {
            1
        } else if distance <= policy.far_distance {
            policy.medium_stride
        } else {
            policy.far_stride
        };
        let due = draw && tick % stride == (actor.reference.object_id as u64) % stride;
        rows.push((
            actor.requires_endpoint,
            distance,
            due,
            LodDecision {
                reference: actor.reference,
                draw,
                update_pose: false,
                stride,
            },
        ));
    }
    rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.3.reference.cmp(&b.3.reference))
    });
    let mut used = 0;
    for (endpoint, _, due, decision) in &mut rows {
        if (*endpoint || *due) && used < policy.max_pose_updates {
            decision.update_pose = true;
            used += 1;
        }
    }
    let mut result: Vec<_> = rows.into_iter().map(|r| r.3).collect();
    result.sort_by_key(|d| d.reference);
    Ok(result)
}
