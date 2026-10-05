// SPDX-License-Identifier: MPL-2.0
//! Source-preserving standalone Vitaboy writers. The original source writer
//! normalizes some flags and reserved fields; these writers retain every field
//! represented by our decoder and reject inconsistent or lossy edits.

use super::*;

pub(crate) struct Writer<'a> {
    limits: &'a Limits,
    bytes: Option<Vec<u8>>,
    size: usize,
    retained: usize,
}

impl<'a> Writer<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            bytes: None,
            size: 0,
            retained: 0,
        }
    }
    fn check(&self) -> Result<()> {
        self.limits.check_count(
            self.size,
            self.limits.max_input_bytes,
            0,
            "Vitaboy output bytes",
        )?;
        self.limits.check_count(
            self.size,
            self.limits.max_resource_bytes,
            0,
            "Vitaboy output resource",
        )?;
        let total = self
            .size
            .checked_add(self.retained)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "Vitaboy authoring budget"))?;
        self.limits.check_count(
            total,
            self.limits.max_total_decoded_bytes,
            0,
            "Vitaboy retained and output allocation",
        )
    }
    pub(crate) fn retain<T>(&mut self, capacity: usize) -> Result<()> {
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "Vitaboy retained allocation"))?;
        self.retained = self
            .retained
            .checked_add(bytes)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "Vitaboy retained allocation"))?;
        self.check()
    }
    pub(crate) fn put(&mut self, value: &[u8]) -> Result<()> {
        self.size = self
            .size
            .checked_add(value.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, self.size, "Vitaboy encoded size"))?;
        self.check()?;
        if let Some(bytes) = &mut self.bytes {
            bytes.extend_from_slice(value);
        }
        Ok(())
    }
    pub(crate) fn u8(&mut self, value: u8) -> Result<()> {
        self.put(&[value])
    }
    pub(crate) fn u16(&mut self, value: u16) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    pub(crate) fn u32(&mut self, value: u32) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    pub(crate) fn i32(&mut self, value: i32) -> Result<()> {
        self.u32(value as u32)
    }
    pub(crate) fn float(&mut self, value: F32Bits) -> Result<()> {
        if !value.is_finite() {
            return Err(invalid(self.size, "non-finite binary32 value"));
        }
        self.put(&value.0.to_le_bytes())
    }
    pub(crate) fn string(&mut self, value: &String, long: bool) -> Result<()> {
        self.limits.check_count(
            value.len(),
            self.limits.max_string_bytes,
            self.size,
            "Vitaboy string bytes",
        )?;
        if !value.is_ascii() {
            return Err(unsupported(self.size, "non-ASCII Vitaboy string"));
        }
        self.retain::<u8>(value.capacity())?;
        if long {
            let n = i16::try_from(value.len())
                .map_err(|_| invalid(self.size, "long Pascal string exceeds i16"))?;
            self.u16(n as u16)?;
        } else {
            let n = u8::try_from(value.len())
                .map_err(|_| invalid(self.size, "Pascal string exceeds u8"))?;
            self.u8(n)?;
        }
        self.put(value.as_bytes())
    }
    pub(crate) fn count(&mut self, count: usize, limit: usize, name: &str) -> Result<()> {
        self.limits.check_count(count, limit, self.size, name)?;
        let count = u32::try_from(count).map_err(|_| invalid(self.size, "count exceeds u32"))?;
        self.u32(count)
    }
    pub(crate) fn version(&mut self, actual: u32, expected: u32) -> Result<()> {
        if actual != expected {
            return Err(unsupported(0, "unsupported standalone Vitaboy version"));
        }
        self.u32(actual)
    }
    fn vector(&mut self, value: Vector3Bits, policy: CoordinatePolicy) -> Result<()> {
        for value in policy.vector(value) {
            self.float(value)?;
        }
        Ok(())
    }
    fn quaternion(&mut self, value: QuaternionBits, policy: CoordinatePolicy) -> Result<()> {
        for value in policy.quaternion(value) {
            self.float(value)?;
        }
        Ok(())
    }
    fn properties(&mut self, properties: &PropertyList) -> Result<()> {
        self.retain::<PropertyItem>(properties.items.capacity())?;
        self.count(
            properties.items.len(),
            self.limits.max_entries,
            "property items",
        )?;
        for item in &properties.items {
            self.retain::<(String, String)>(item.pairs.capacity())?;
            self.count(item.pairs.len(), self.limits.max_entries, "property pairs")?;
            for (key, value) in &item.pairs {
                self.string(key, false)?;
                self.string(value, false)?;
            }
        }
        Ok(())
    }
    pub(crate) fn file_key(&mut self, value: FileKey) -> Result<()> {
        self.u32(value.file_id)?;
        self.u32(value.type_id)
    }
}

pub(crate) fn encode(
    limits: &Limits,
    write: impl Fn(&mut Writer<'_>) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut plan = Writer::new(limits);
    write(&mut plan)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(plan.size)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "Vitaboy output allocation"))?;
    let expected = plan.size;
    let mut output = Writer {
        bytes: Some(bytes),
        ..Writer::new(limits)
    };
    write(&mut output)?;
    debug_assert_eq!(output.size, expected);
    Ok(output.bytes.expect("output writer owns bytes"))
}

/// Validate an authoring traversal and return (encoded bytes, retained bytes)
/// without allocating an output. Container writers use this to admit nested
/// payload and envelope allocations together.
pub(crate) fn measure(
    limits: &Limits,
    write: impl Fn(&mut Writer<'_>) -> Result<()>,
) -> Result<(usize, usize)> {
    let mut plan = Writer::new(limits);
    write(&mut plan)?;
    Ok((plan.size, plan.retained))
}

/// Encode the exact standalone v2 mixed-endian layout. Face/vertex/binding
/// order, raw blend weights and zero-normal bits are retained.
pub fn encode_mesh(mesh: &Mesh, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| write_mesh(w, mesh, limits))
}

pub(crate) fn write_mesh(w: &mut Writer<'_>, mesh: &Mesh, limits: &Limits) -> Result<()> {
    w.version(mesh.version, 2)?;
    w.retain::<String>(mesh.bone_names.capacity())?;
    w.count(
        mesh.bone_names.len(),
        limits.max_entries.min(i32::MAX as usize),
        "mesh bone names",
    )?;
    for name in &mesh.bone_names {
        w.string(name, false)?;
    }
    write_mesh_geometry(w, mesh, limits)
}

/// Shared geometry traversal, independent of standalone/legacy string layout.
pub(crate) fn write_mesh_geometry(w: &mut Writer<'_>, mesh: &Mesh, limits: &Limits) -> Result<()> {
    w.retain::<[i32; 3]>(mesh.faces.capacity())?;
    w.count(
        mesh.faces.len(),
        limits.max_entries.min(i32::MAX as usize),
        "mesh faces",
    )?;
    for face in &mesh.faces {
        for &index in face {
            if index < 0 || index as usize >= mesh.vertices.len() {
                return Err(invalid(w.size, "face index outside real vertices"));
            }
            w.i32(index)?;
        }
    }
    w.retain::<BoneBinding>(mesh.bindings.capacity())?;
    w.count(
        mesh.bindings.len(),
        limits.max_entries.min(i32::MAX as usize),
        "mesh bindings",
    )?;
    for binding in &mesh.bindings {
        if binding.bone_index < 0
            || binding.bone_index as usize >= mesh.bone_names.len()
            || binding.real_vertex_count < 0
            || binding.blend_vertex_count < 0
        {
            return Err(invalid(w.size, "invalid mesh binding"));
        }
        index_range(
            binding.first_real_vertex,
            binding.real_vertex_count as usize,
            mesh.vertices.len(),
            w.size,
            "real vertex binding range",
        )?;
        index_range(
            binding.first_blend_vertex,
            binding.blend_vertex_count as usize,
            mesh.blend_vertices.len(),
            w.size,
            "blend vertex binding range",
        )?;
        for field in [
            binding.bone_index,
            binding.first_real_vertex,
            binding.real_vertex_count,
            binding.first_blend_vertex,
            binding.blend_vertex_count,
        ] {
            w.i32(field)?;
        }
    }
    let total = mesh
        .vertices
        .len()
        .checked_add(mesh.blend_vertices.len())
        .ok_or_else(|| invalid(w.size, "total vertex overflow"))?;
    limits.check_count(total, limits.max_vertices, w.size, "total vertices")?;
    w.retain::<MeshVertex>(mesh.vertices.capacity())?;
    w.count(
        mesh.vertices.len(),
        limits.max_vertices.min(i32::MAX as usize),
        "real vertices",
    )?;
    for vertex in &mesh.vertices {
        for value in vertex.texture_coordinate {
            w.float(value)?;
        }
    }
    w.retain::<BlendVertex>(mesh.blend_vertices.capacity())?;
    w.count(
        mesh.blend_vertices.len(),
        limits.max_vertices.min(i32::MAX as usize),
        "blend vertices",
    )?;
    for vertex in &mesh.blend_vertices {
        if vertex.other_vertex < 0 || vertex.other_vertex as usize >= mesh.vertices.len() {
            return Err(invalid(w.size, "blend destination outside real vertices"));
        }
        w.i32(vertex.raw_weight)?;
        w.i32(vertex.other_vertex)?;
    }
    if mesh.repeated_real_vertex_count < 0
        || mesh.repeated_real_vertex_count as usize != mesh.vertices.len()
    {
        return Err(invalid(w.size, "inconsistent repeated vertex count"));
    }
    w.i32(mesh.repeated_real_vertex_count)?;
    for vertex in &mesh.vertices {
        w.vector(vertex.position, mesh.coordinate_policy)?;
        w.vector(vertex.normal, mesh.coordinate_policy)?;
    }
    for vertex in &mesh.blend_vertices {
        w.vector(vertex.position, mesh.coordinate_policy)?;
        w.vector(vertex.normal, mesh.coordinate_policy)?;
    }
    Ok(())
}

/// Encode standalone animation v2 with exact flags, motion and event order.
/// Derived frame counts and hidden property data must agree with the wire data.
pub fn encode_animation(animation: &Animation, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| write_animation(w, animation, limits))
}

pub(crate) fn write_animation(
    w: &mut Writer<'_>,
    animation: &Animation,
    limits: &Limits,
) -> Result<()> {
    w.version(animation.version, 2)?;
    w.string(&animation.name, true)?;
    if animation.duration_ms.get() < 0.0 {
        return Err(invalid(w.size, "negative animation duration"));
    }
    w.float(animation.duration_ms)?;
    w.float(animation.distance)?;
    w.u8(animation.is_moving)?;
    w.retain::<Vector3Bits>(animation.translations.capacity())?;
    w.count(
        animation.translations.len(),
        limits.max_frames,
        "animation translations",
    )?;
    for &value in &animation.translations {
        w.vector(value, animation.coordinate_policy)?;
    }
    w.retain::<QuaternionBits>(animation.rotations.capacity())?;
    w.count(
        animation.rotations.len(),
        limits.max_frames,
        "animation rotations",
    )?;
    for &value in &animation.rotations {
        w.quaternion(value, animation.coordinate_policy)?;
    }
    w.retain::<Motion>(animation.motions.capacity())?;
    w.count(
        animation.motions.len(),
        limits.max_entries,
        "animation motions",
    )?;
    let frames = animation
        .motions
        .iter()
        .map(|m| m.frame_count)
        .max()
        .unwrap_or(0);
    if frames != animation.num_frames {
        return Err(invalid(
            w.size,
            "inconsistent derived animation frame count",
        ));
    }
    for motion in &animation.motions {
        w.u32(motion.unknown)?;
        w.string(&motion.bone_name, false)?;
        w.count(
            motion.frame_count as usize,
            limits.max_frames,
            "motion frame count",
        )?;
        if motion.duration_ms.get() < 0.0 {
            return Err(invalid(w.size, "negative motion duration"));
        }
        w.float(motion.duration_ms)?;
        w.u8(motion.translation_flag)?;
        w.u8(motion.rotation_flag)?;
        if motion.translation_flag == 1 {
            index_range(
                motion.first_translation_index,
                motion.frame_count as usize,
                animation.translations.len(),
                w.size,
                "motion translation indices",
            )?;
        }
        if motion.rotation_flag == 1 {
            index_range(
                motion.first_rotation_index,
                motion.frame_count as usize,
                animation.rotations.len(),
                w.size,
                "motion rotation indices",
            )?;
        }
        w.i32(motion.first_translation_index)?;
        w.i32(motion.first_rotation_index)?;
        w.u8(motion.properties_flag)?;
        w.retain::<PropertyList>(motion.properties.capacity())?;
        if motion.properties_flag == 1 {
            w.count(
                motion.properties.len(),
                limits.max_entries,
                "motion property lists",
            )?;
            for properties in &motion.properties {
                w.properties(properties)?;
            }
        } else if !motion.properties.is_empty() {
            return Err(invalid(w.size, "motion property flag would discard data"));
        }
        w.u8(motion.time_properties_flag)?;
        w.retain::<TimePropertyList>(motion.time_properties.capacity())?;
        if motion.time_properties_flag == 1 {
            w.count(
                motion.time_properties.len(),
                limits.max_entries,
                "time property lists",
            )?;
            for list in &motion.time_properties {
                w.retain::<TimeProperty>(list.items.capacity())?;
                w.count(list.items.len(), limits.max_entries, "time properties")?;
                for item in &list.items {
                    w.i32(item.id)?;
                    w.properties(&item.properties)?;
                }
            }
        } else if !motion.time_properties.is_empty() {
            return Err(invalid(w.size, "time property flag would discard data"));
        }
    }
    Ok(())
}

pub(crate) fn validate_skeleton(s: &Skeleton, limits: &Limits, w: &mut Writer<'_>) -> Result<()> {
    let n = s.bones.len();
    if n == 0 || n > i16::MAX as usize || s.root >= n {
        return Err(invalid(0, "invalid skeleton bone count/root"));
    }
    limits.check_count(n, limits.max_entries, 0, "skeleton bones")?;
    // Conservative BTreeMap node/key allocation and visitation scratch, charged
    // before constructing either. No recursive traversal or unbounded stack.
    w.retain::<u8>(
        n.checked_mul(128)
            .ok_or_else(|| invalid(0, "hierarchy scratch overflow"))?,
    )?;
    let mut names = BTreeMap::new();
    for (index, bone) in s.bones.iter().enumerate() {
        if bone.index != index || bone.name.is_empty() || bone.name == "NULL" {
            return Err(invalid(0, "invalid bone index/name"));
        }
        if names.insert(bone.name.as_str(), index).is_some() {
            return Err(Error::new(ErrorKind::Duplicate, 0, "duplicate bone name"));
        }
    }
    let mut seen_child = vec![false; n];
    for (index, bone) in s.bones.iter().enumerate() {
        let parent = if bone.parent_name == "NULL" {
            None
        } else {
            Some(
                *names
                    .get(bone.parent_name.as_str())
                    .ok_or_else(|| invalid(0, "missing bone parent"))?,
            )
        };
        if bone.parent != parent || (parent.is_none() != (index == s.root)) {
            return Err(invalid(0, "inconsistent skeleton parent/root metadata"));
        }
        for &child in &bone.children {
            if child >= n || seen_child[child] || s.bones[child].parent != Some(index) {
                return Err(invalid(0, "inconsistent skeleton child metadata"));
            }
            seen_child[child] = true;
        }
        let mut cursor = Some(index);
        let mut depth = 0usize;
        while let Some(current) = cursor {
            if current >= n {
                return Err(invalid(0, "invalid skeleton parent index"));
            }
            depth += 1;
            if depth > n {
                return Err(invalid(0, "cyclic skeleton hierarchy"));
            }
            limits.check_count(depth, limits.max_depth, 0, "skeleton hierarchy depth")?;
            cursor = s.bones[current].parent;
        }
    }
    if seen_child
        .iter()
        .enumerate()
        .any(|(i, &seen)| seen != (i != s.root))
    {
        return Err(invalid(0, "incomplete skeleton child metadata"));
    }
    Ok(())
}

/// Encode standalone skeleton v1, checking that derived hierarchy metadata
/// agrees with names and source order instead of dropping contradictory edits.
pub fn encode_skeleton(skeleton: &Skeleton, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| write_skeleton(w, skeleton, limits))
}

pub(crate) fn write_skeleton(
    w: &mut Writer<'_>,
    skeleton: &Skeleton,
    limits: &Limits,
) -> Result<()> {
    w.version(skeleton.version, 1)?;
    w.retain::<Bone>(skeleton.bones.capacity())?;
    validate_skeleton(skeleton, limits, w)?;
    w.string(&skeleton.name, false)?;
    w.u16(skeleton.bones.len() as u16)?;
    for bone in &skeleton.bones {
        w.retain::<usize>(bone.children.capacity())?;
        w.i32(bone.unknown)?;
        w.string(&bone.name, false)?;
        w.string(&bone.parent_name, false)?;
        w.u8(bone.properties_flag)?;
        if bone.properties_flag != 0 {
            w.properties(&bone.properties)?;
        } else if !bone.properties.items.is_empty() {
            return Err(invalid(w.size, "bone property flag would discard data"));
        } else {
            w.retain::<PropertyItem>(bone.properties.items.capacity())?;
        }
        w.vector(bone.translation, skeleton.coordinate_policy)?;
        w.quaternion(bone.rotation, skeleton.coordinate_policy)?;
        w.i32(bone.can_translate)?;
        w.i32(bone.can_rotate)?;
        w.i32(bone.can_blend)?;
        w.float(bone.wiggle_value)?;
        w.float(bone.wiggle_power)?;
    }
    Ok(())
}

pub fn encode_binding(binding: &Binding, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| {
        w.version(binding.version, 1)?;
        w.string(&binding.bone, false)?;
        for (selector, key) in [
            (binding.mesh_selector, binding.mesh),
            (binding.texture_selector, binding.texture),
        ] {
            w.u32(selector)?;
            match (selector, key) {
                (0, None) => {}
                (8, Some(key)) => {
                    w.u32(key.group_id)?;
                    w.u32(key.file_id)?;
                    w.u32(key.type_id)?;
                }
                _ => return Err(invalid(w.size, "binding selector and reference disagree")),
            }
        }
        Ok(())
    })
}

pub fn encode_appearance(appearance: &Appearance, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| {
        w.version(appearance.version, 1)?;
        w.file_key(appearance.thumbnail)?;
        w.retain::<FileKey>(appearance.bindings.capacity())?;
        w.count(
            appearance.bindings.len(),
            limits.max_entries,
            "appearance bindings",
        )?;
        for &binding in &appearance.bindings {
            w.file_key(binding)?;
        }
        Ok(())
    })
}

pub fn encode_outfit(outfit: &Outfit, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| {
        w.version(outfit.version, 1)?;
        w.u32(outfit.unknown)?;
        w.file_key(outfit.light_appearance)?;
        w.file_key(outfit.medium_appearance)?;
        w.file_key(outfit.dark_appearance)?;
        w.u32(outfit.hand_group)?;
        w.u32(outfit.region)
    })
}
