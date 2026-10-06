use crate::normalized::{quat, vec3};
use crate::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use wonderland_render_core::{math::*, AssetKey};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalPose {
    pub translation: Vec3,
    pub rotation: Quat,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub locals: Vec<LocalPose>,
    pub palette: Vec<Mat4>,
    pub rig_key: AssetKey,
}
/// Validated skeleton whose identity and bone layout cannot be retagged in place.
///
/// ```compile_fail,E0616
/// use wonderland_avatar_view::fixtures;
/// use wonderland_render_core::AssetKey;
/// let mut rig = fixtures::representative_rig();
/// rig.key = AssetKey([0; 32]);
/// ```
#[derive(Clone, Debug)]
pub struct Rig {
    source: Skeleton,
    key: AssetKey,
    traversal: Vec<usize>,
}
impl Rig {
    pub fn new(source: Skeleton, resource: AssetKey, limits: AvatarLimits) -> Result<Self> {
        let n = source.bones.len();
        if n == 0 || n > limits.max_bones {
            return Err(AvatarError::Limit("palette"));
        }
        if source.coordinate_policy != CoordinatePolicy::FreeSo || source.version != 1 {
            return Err(AvatarError::Invalid("normalized skeleton policy/version"));
        }
        if source.root >= n || !source.bones[source.root].name.eq_ignore_ascii_case("ROOT") {
            return Err(AvatarError::Invalid("runtime root must be ROOT"));
        }
        let mut names = BTreeSet::new();
        let mut folded = BTreeSet::new();
        let mut roots = 0;
        for (i, b) in source.bones.iter().enumerate() {
            if b.index != i
                || b.name.is_empty()
                || !b.name.is_ascii()
                || b.name.len() > 1024
                || !names.insert(b.name.clone())
                || !folded.insert(b.name.to_ascii_lowercase())
            {
                return Err(AvatarError::Invalid("bone names/indices"));
            }
            if !vec3(b.translation).is_finite()
                || !unit(quat(b.rotation))
                || !b.wiggle_value.get().is_finite()
                || !b.wiggle_power.get().is_finite()
            {
                return Err(AvatarError::Invalid("nonfinite/nonunit bone"));
            }
            match b.parent {
                None => {
                    roots += 1;
                    if i != source.root || b.parent_name != "NULL" {
                        return Err(AvatarError::Invalid("root parent"));
                    }
                }
                Some(p) => {
                    if p >= n || p == i || source.bones[p].name != b.parent_name {
                        return Err(AvatarError::Invalid("parent reference"));
                    }
                }
            }
            let expected: Vec<_> = source
                .bones
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent == Some(i))
                .map(|(j, _)| j)
                .collect();
            if b.children != expected {
                return Err(AvatarError::Invalid("children must preserve file order"));
            }
        }
        if roots != 1 {
            return Err(AvatarError::Invalid("one root required"));
        }
        let mut traversal = Vec::with_capacity(n);
        let mut stack = vec![source.root];
        let mut seen = vec![false; n];
        while let Some(i) = stack.pop() {
            if seen[i] {
                return Err(AvatarError::Invalid("hierarchy cycle"));
            }
            seen[i] = true;
            traversal.push(i);
            for &c in source.bones[i].children.iter().rev() {
                stack.push(c);
            }
        }
        if traversal.len() != n {
            return Err(AvatarError::Invalid("unreachable/cyclic bone"));
        }
        if bincode::serialized_size(&source).map_err(|_| AvatarError::Invalid("rig size"))?
            > limits.max_metadata_bytes as u64
        {
            return Err(AvatarError::Limit("rig metadata"));
        }
        let encoded =
            bincode::serialize(&source).map_err(|_| AvatarError::Invalid("rig encoding"))?;
        if encoded.len() > limits.max_metadata_bytes {
            return Err(AvatarError::Limit("rig metadata"));
        }
        let mut h = Sha256::new();
        h.update(b"C-avatar-rig-v1");
        h.update(resource.0);
        h.update(encoded);
        let rig = Self {
            source,
            key: AssetKey(h.finalize().into()),
            traversal,
        };
        let mut pose = Pose {
            locals: rig
                .source
                .bones
                .iter()
                .map(|b| LocalPose {
                    translation: vec3(b.translation),
                    rotation: quat(b.rotation),
                })
                .collect(),
            palette: vec![],
            rig_key: rig.key,
        };
        pose.rebuild(&rig)?;
        Ok(rig)
    }
    pub fn key(&self) -> AssetKey {
        self.key
    }
    pub fn source(&self) -> &Skeleton {
        &self.source
    }
    pub fn traversal(&self) -> &[usize] {
        &self.traversal
    }
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.source.bones.iter().position(|b| b.name == name)
    }
    pub fn bind_pose(&self) -> Pose {
        let mut p = Pose {
            locals: self
                .source
                .bones
                .iter()
                .map(|b| LocalPose {
                    translation: vec3(b.translation),
                    rotation: quat(b.rotation),
                })
                .collect(),
            palette: vec![Mat4::IDENTITY; self.source.bones.len()],
            rig_key: self.key,
        };
        p.rebuild(self).expect("validated rig bind pose");
        p
    }
}
impl Pose {
    pub fn rebuild(&mut self, rig: &Rig) -> Result<()> {
        if self.rig_key != rig.key || self.locals.len() != rig.source.bones.len() {
            return Err(AvatarError::Invalid("pose/rig mismatch"));
        }
        if self
            .locals
            .iter()
            .any(|l| !l.translation.is_finite() || !unit(l.rotation))
        {
            return Err(AvatarError::Invalid("nonfinite/nonunit pose"));
        }
        let mut palette = vec![Mat4::IDENTITY; self.locals.len()];
        for &i in &rig.traversal {
            let l = self.locals[i];
            let local = Mat4::from_translation(l.translation) * Mat4::from_quat(l.rotation);
            palette[i] = match rig.source.bones[i].parent {
                Some(p) => palette[p] * local,
                None => local,
            };
            if !matrix_finite(palette[i]) {
                return Err(AvatarError::Invalid("palette overflow"));
            }
        }
        self.palette = palette;
        Ok(())
    }
}
pub(crate) fn matrix_finite(m: Mat4) -> bool {
    m.cols.iter().flatten().all(|v| v.is_finite())
}
