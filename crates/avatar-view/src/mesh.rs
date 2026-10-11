use crate::normalized::vec3;
use crate::rig::matrix_finite;
use crate::*;
use sha2::{Digest, Sha256};
use wonderland_render_core::{math::*, AssetKey, Mesh, Vertex};
/// Source runtime transforms only the primary normal; blend normal is retained metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalPolicy {
    SourcePrimary,
}
#[derive(Clone, Debug)]
pub struct PreparedVertex {
    pub primary_position: Vec3,
    pub secondary_position: Vec3,
    pub primary_normal: Vec3,
    pub secondary_normal: Vec3,
    pub primary_joint: usize,
    pub secondary_joint: usize,
    pub weight: f32,
    pub uv: Vec2,
}
#[derive(Clone, Debug)]
pub struct PreparedMesh {
    pub vertices: Vec<PreparedVertex>,
    pub indices: Vec<u32>,
    pub cache_key: AssetKey,
    pub rig_key: AssetKey,
    pub normal_policy: NormalPolicy,
}
pub(crate) fn range(first: i32, count: i32, total: usize) -> Result<std::ops::Range<usize>> {
    if count < 0 {
        return Err(AvatarError::Invalid("negative count"));
    }
    if count == 0 {
        return Ok(0..0);
    }
    if first < 0 {
        return Err(AvatarError::Invalid("negative range"));
    }
    let start = first as usize;
    let end = start
        .checked_add(count as usize)
        .ok_or(AvatarError::Invalid("range overflow"))?;
    if end > total {
        return Err(AvatarError::Invalid("range out of bounds"));
    }
    Ok(start..end)
}
impl PreparedMesh {
    pub fn prepare(
        rig: &Rig,
        mesh: &SourceMesh,
        resource: AssetKey,
        limits: AvatarLimits,
    ) -> Result<Self> {
        if mesh.coordinate_policy != CoordinatePolicy::FreeSo || mesh.version != 2 {
            return Err(AvatarError::Invalid("normalized mesh policy/version"));
        }
        if mesh.vertices.is_empty()
            || mesh.vertices.len() > limits.max_vertices
            || mesh.blend_vertices.len() > limits.max_blends
            || mesh.faces.len() > limits.max_indices / 3
            || mesh.bindings.len() > limits.max_motions
            || mesh.bone_names.len() > limits.max_bones
        {
            return Err(AvatarError::Limit("mesh"));
        }
        if bincode::serialized_size(mesh).map_err(|_| AvatarError::Invalid("mesh size"))?
            > limits.max_metadata_bytes as u64
        {
            return Err(AvatarError::Limit("mesh metadata"));
        }
        if mesh
            .bone_names
            .iter()
            .any(|n| n.len() > 1024 || !n.is_ascii())
        {
            return Err(AvatarError::Invalid("mesh bone names"));
        }
        // Native Mesh.Read does not use the second on-disk count to size or
        // bind vertices. Preserve it as source metadata; validate actual arrays.
        for b in &mesh.bindings {
            if b.bone_index < 0 || b.bone_index as usize >= mesh.bone_names.len() {
                return Err(AvatarError::Invalid("mesh bone index"));
            }
            let name = &mesh.bone_names[b.bone_index as usize];
            if !rig
                .source()
                .bones
                .iter()
                .any(|bone| bone.name.eq_ignore_ascii_case(name))
            {
                return Err(AvatarError::MissingBone(name.clone()));
            }
            range(
                b.first_real_vertex,
                b.real_vertex_count,
                mesh.vertices.len(),
            )?;
            range(
                b.first_blend_vertex,
                b.blend_vertex_count,
                mesh.blend_vertices.len(),
            )?;
        }
        let mut vertices = Vec::with_capacity(mesh.vertices.len());
        for v in &mesh.vertices {
            let position = vec3(v.position);
            let normal = vec3(v.normal);
            let uv = Vec2::new(v.texture_coordinate[0].get(), v.texture_coordinate[1].get());
            if !position.is_finite() || !normal.is_finite() || !uv.is_finite() {
                return Err(AvatarError::Invalid("nonfinite vertex"));
            }
            vertices.push(PreparedVertex {
                primary_position: position,
                secondary_position: Vec3::ZERO,
                primary_normal: if normal == Vec3::ZERO {
                    Vec3::Y
                } else {
                    normal
                },
                secondary_normal: Vec3::ZERO,
                primary_joint: rig.source().root,
                secondary_joint: rig.source().root,
                weight: 0.0,
                uv,
            });
        }
        let mut primary_assigned = vec![false; vertices.len()];
        let mut blend_joints = vec![None; mesh.blend_vertices.len()];
        for &bone in rig.traversal() {
            for b in &mesh.bindings {
                if mesh.bone_names[b.bone_index as usize]
                    .eq_ignore_ascii_case(&rig.source().bones[bone].name)
                {
                    for i in range(b.first_real_vertex, b.real_vertex_count, vertices.len())? {
                        vertices[i].primary_joint = bone;
                        primary_assigned[i] = true;
                    }
                    for i in range(
                        b.first_blend_vertex,
                        b.blend_vertex_count,
                        blend_joints.len(),
                    )? {
                        blend_joints[i] = Some(bone);
                    }
                }
            }
        }
        if primary_assigned.iter().any(|a| !*a) {
            return Err(AvatarError::Invalid("unbound real vertex"));
        }
        for (i, b) in mesh.blend_vertices.iter().enumerate() {
            if b.other_vertex < 0 || b.other_vertex as usize >= vertices.len() {
                return Err(AvatarError::Invalid("blend destination"));
            }
            let p = vec3(b.position);
            let n = vec3(b.normal);
            if !p.is_finite() || !n.is_finite() {
                return Err(AvatarError::Invalid("nonfinite blend"));
            }
            let v = &mut vertices[b.other_vertex as usize];
            v.secondary_joint =
                blend_joints[i].ok_or(AvatarError::Invalid("unbound blend vertex"))?;
            v.secondary_position = p;
            v.secondary_normal = n;
            v.weight = b.raw_weight as f32 / 32768.0;
        }
        let mut indices = Vec::with_capacity(mesh.faces.len() * 3);
        for f in &mesh.faces {
            for &i in f {
                if i < 0 || i as usize >= vertices.len() {
                    return Err(AvatarError::Invalid("face index"));
                }
                indices.push(i as u32);
            }
        }
        let mut h = Sha256::new();
        h.update(b"C-avatar-dual-primary-normal-v1");
        h.update(resource.0);
        h.update(rig.key.0);
        h.update(bincode::serialize(mesh).map_err(|_| AvatarError::Invalid("mesh encoding"))?);
        Ok(Self {
            vertices,
            indices,
            cache_key: AssetKey(h.finalize().into()),
            rig_key: rig.key,
            normal_policy: NormalPolicy::SourcePrimary,
        })
    }
    pub fn skin(&self, pose: &Pose, world: Mat4) -> Result<Mesh> {
        if self.vertices.is_empty()
            || self.indices.len() % 3 != 0
            || self
                .indices
                .iter()
                .any(|i| *i as usize >= self.vertices.len())
            || self
                .vertices
                .iter()
                .any(|v| !v.uv.is_finite() || !v.weight.is_finite())
        {
            return Err(AvatarError::Invalid("prepared mesh indices/attributes"));
        }
        if pose.rig_key != self.rig_key
            || !matrix_finite(world)
            || pose.palette.iter().any(|m| !matrix_finite(*m))
        {
            return Err(AvatarError::Invalid("skin palette/world"));
        }
        let mut vertices = Vec::with_capacity(self.vertices.len());
        for v in &self.vertices {
            let a = *pose
                .palette
                .get(v.primary_joint)
                .ok_or(AvatarError::Invalid("primary palette index"))?;
            let b = *pose
                .palette
                .get(v.secondary_joint)
                .ok_or(AvatarError::Invalid("secondary palette index"))?;
            let position = world.transform_point3(
                a.transform_point3(v.primary_position) * (1.0 - v.weight)
                    + b.transform_point3(v.secondary_position) * v.weight,
            );
            let normal = world.transform_vector3(a.transform_vector3(v.primary_normal));
            if !position.is_finite() || !normal.is_finite() {
                return Err(AvatarError::Invalid("skinning overflow"));
            }
            vertices.push(Vertex {
                position,
                normal,
                uv: v.uv,
                color: [1.0; 4],
            });
        }
        Ok(Mesh {
            vertices,
            indices: self.indices.clone(),
        })
    }
    pub fn bounds(&self, pose: &Pose, world: Mat4) -> Result<Aabb> {
        let mesh = self.skin(pose, world)?;
        Aabb::from_points(&mesh.vertices.iter().map(|v| v.position).collect::<Vec<_>>())
            .ok_or(AvatarError::Invalid("empty bounds"))
    }
    pub fn resident_bytes(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<PreparedVertex>() + self.indices.len() * 4
    }
}

impl PreparedMesh {
    /// Conservative rotation-independent envelope for bind pose and all supplied
    /// channels, including arbitrary unit rotations and unclamped skin weights.
    /// Channel mixing must be convex (nonnegative layer weights); use exact bounds
    /// for externally authored/extrapolated poses outside this admitted envelope.
    pub fn conservative_animation_bounds(
        &self,
        rig: &Rig,
        clips: &[&Clip],
        world: Mat4,
    ) -> Result<Aabb> {
        if self.rig_key != rig.key
            || !matrix_finite(world)
            || clips.iter().any(|c| c.rig_key != rig.key)
        {
            return Err(AvatarError::Invalid("envelope rig/world"));
        }
        let mut translations: Vec<_> = rig
            .source()
            .bones
            .iter()
            .map(|b| vec3(b.translation).length())
            .collect();
        // Validated quaternion norm tolerance is .001. 1.005 is a conservative
        // per-joint stretch bound, also leaving room for binary32 accumulation.
        for clip in clips {
            let source = clip.source();
            for m in &source.motions {
                if let Some(bone) = rig.bone_index(&m.bone_name) {
                    if m.translation_flag == 1 {
                        for i in crate::mesh::range(
                            m.first_translation_index,
                            m.frame_count as i32,
                            source.translations.len(),
                        )? {
                            translations[bone] =
                                translations[bone].max(vec3(source.translations[i]).length());
                        }
                    }
                }
            }
        }
        let mut reach = vec![0.0; translations.len()];
        let mut stretch = vec![1.0; translations.len()];
        for &i in rig.traversal() {
            match rig.source().bones[i].parent {
                None => {
                    reach[i] = translations[i];
                    stretch[i] = 1.005;
                }
                Some(parent) => {
                    reach[i] = reach[parent] + stretch[parent] * translations[i];
                    stretch[i] = stretch[parent] * 1.005;
                }
            }
        }
        let mut radius = 0.0f32;
        for vertex in &self.vertices {
            let a = vertex.primary_joint;
            let b = vertex.secondary_joint;
            if a >= reach.len() || b >= reach.len() {
                return Err(AvatarError::Invalid("envelope joint"));
            }
            let primary = reach[a] + stretch[a] * vertex.primary_position.length();
            let secondary = reach[b] + stretch[b] * vertex.secondary_position.length();
            radius =
                radius.max((1.0 - vertex.weight).abs() * primary + vertex.weight.abs() * secondary);
        }
        radius = radius * 1.0001 + 0.0001;
        if !radius.is_finite() {
            return Err(AvatarError::Invalid("envelope overflow"));
        }
        Aabb::new(Vec3::ONE * (-radius), Vec3::ONE * radius)
            .and_then(|b| b.transformed(world))
            .ok_or(AvatarError::Invalid("world envelope"))
    }
}
