//! Bounded metadata readers for the standalone TSO Vitaboy binary resources.
//!
//! Source: `tso.vitaboy.model/{Animation,Skeleton,Mesh,Binding,Appearance,Outfit}.cs`
//! and `tso.files/Utils/IoBuffer.cs`, revision
//! `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
//! These files use **big-endian integers and little-endian floats**: the source
//! `IoBuffer.ReadFloat` ignores its integer byte-order setting. No floats are
//! widened to f64, normalized, sorted, or silently rounded during import.

use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, mem::size_of};

pub(crate) mod encode;
pub use encode::{
    encode_animation, encode_appearance, encode_binding, encode_mesh, encode_outfit,
    encode_skeleton,
};
mod references;
pub use references::{
    decode_collection, decode_hand_group, decode_purchasable_outfit, encode_collection,
    encode_hand_group, encode_purchasable_outfit, Collection, CollectionItem, HandGroup,
    PurchasableOutfit,
};
mod legacy;
pub use legacy::{
    decode_bcf, decode_bmf, decode_cfp, encode_bcf, encode_bcf_text, encode_bmf, encode_bmf_text,
    encode_cfp, Bcf, BcfAnimation, BcfAppearance, BcfBinding, BcfMotion, BcfSkeleton, CfpFrames,
    LegacyEncoding, LegacyMesh, LegacyTextOutput, LegacyTextPolicy, SkippedBone,
};

/// IEEE-754 binary32 bits, serialized as an integer, including the sign of zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct F32Bits(pub u32);

impl F32Bits {
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
    pub fn from_f32(value: f32) -> Self {
        Self(value.to_bits())
    }
    pub fn negated(self) -> Self {
        Self(self.0 ^ 0x8000_0000)
    }
    pub fn is_finite(self) -> bool {
        self.0 & 0x7f80_0000 != 0x7f80_0000
    }
}

pub type Vector3Bits = [F32Bits; 3];
pub type QuaternionBits = [F32Bits; 4];

/// FreeSO's import conversion: negate vector X and quaternion Y, Z, and W.
/// This is an involution, so applying it again recovers every original bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinatePolicy {
    Source,
    FreeSo,
}

impl CoordinatePolicy {
    pub fn vector(self, mut value: Vector3Bits) -> Vector3Bits {
        if self == Self::FreeSo {
            value[0] = value[0].negated();
        }
        value
    }
    pub fn quaternion(self, mut value: QuaternionBits) -> QuaternionBits {
        if self == Self::FreeSo {
            for value in &mut value[1..] {
                *value = value.negated();
            }
        }
        value
    }
}

pub(crate) fn invalid(offset: usize, context: &str) -> Error {
    Error::new(ErrorKind::InvalidData, offset, context)
}

pub(crate) fn unsupported(offset: usize, context: &str) -> Error {
    Error::new(ErrorKind::UnsupportedVersion, offset, context)
}

/// Accounts for owned buffers across nested lists, not just each individual list.
pub(crate) struct DecodeBudget<'a> {
    pub limits: &'a Limits,
    total: usize,
}

impl<'a> DecodeBudget<'a> {
    pub fn new(bytes: &[u8], limits: &'a Limits) -> Result<Self> {
        limits.check_input(bytes)?;
        limits.check_count(bytes.len(), limits.max_resource_bytes, 0, "resource bytes")?;
        Ok(Self { limits, total: 0 })
    }
    pub fn reserve<T>(
        &mut self,
        count: usize,
        limit: usize,
        offset: usize,
        label: &str,
    ) -> Result<()> {
        self.limits.check_count(count, limit, offset, label)?;
        let bytes = count
            .checked_mul(size_of::<T>())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, label))?;
        self.total = self
            .total
            .checked_add(bytes)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, label))?;
        self.limits.check_count(
            self.total,
            self.limits.max_total_decoded_bytes,
            offset,
            "total decoded bytes",
        )
    }
}

pub(crate) fn finite_le(reader: &mut Reader<'_>) -> Result<F32Bits> {
    let offset = reader.position();
    let result = F32Bits(reader.u32_le()?);
    if !result.is_finite() {
        return Err(invalid(offset, "non-finite binary32 value"));
    }
    Ok(result)
}

fn vector(reader: &mut Reader<'_>) -> Result<Vector3Bits> {
    Ok(CoordinatePolicy::FreeSo.vector([
        finite_le(reader)?,
        finite_le(reader)?,
        finite_le(reader)?,
    ]))
}

fn quaternion(reader: &mut Reader<'_>) -> Result<QuaternionBits> {
    Ok(CoordinatePolicy::FreeSo.quaternion([
        finite_le(reader)?,
        finite_le(reader)?,
        finite_le(reader)?,
        finite_le(reader)?,
    ]))
}

fn count(
    reader: &mut Reader<'_>,
    budget: &mut DecodeBudget<'_>,
    limit: usize,
    label: &str,
) -> Result<usize> {
    let offset = reader.position();
    let n = reader.u32_be()? as usize;
    budget.limits.check_count(n, limit, offset, label)?;
    Ok(n)
}

fn signed_count(
    reader: &mut Reader<'_>,
    budget: &mut DecodeBudget<'_>,
    limit: usize,
    label: &str,
) -> Result<usize> {
    let offset = reader.position();
    let n = reader.i32_be()?;
    if n < 0 {
        return Err(invalid(offset, label));
    }
    budget
        .limits
        .check_count(n as usize, limit, offset, label)?;
    Ok(n as usize)
}

fn ascii(reader: &mut Reader<'_>, budget: &mut DecodeBudget<'_>, long: bool) -> Result<String> {
    let offset = reader.position();
    let length = if long {
        let n = reader.i16_be()?;
        if n < 0 {
            return Err(invalid(offset, "negative long Pascal string length"));
        }
        n as usize
    } else {
        reader.u8()? as usize
    };
    budget.reserve::<u8>(
        length,
        budget.limits.max_string_bytes,
        offset,
        "Pascal string",
    )?;
    let bytes = reader.read_bytes(length)?;
    // The source maps non-ASCII to '?'. Rejecting them avoids losing original
    // names, property keys, or event values through that lossy replacement.
    if !bytes.is_ascii() {
        return Err(unsupported(offset, "non-ASCII Vitaboy string"));
    }
    Ok(String::from_utf8(bytes.to_vec()).expect("ASCII is valid UTF-8"))
}

fn version(reader: &mut Reader<'_>, expected: u32, name: &str) -> Result<u32> {
    let n = reader.u32_be()?;
    if n != expected {
        return Err(unsupported(0, name));
    }
    Ok(n)
}

fn finish(reader: &Reader<'_>) -> Result<()> {
    if reader.remaining() != 0 {
        return Err(unsupported(
            reader.position(),
            "unrecognized trailing resource data",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyItem {
    pub pairs: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyList {
    pub items: Vec<PropertyItem>,
}

impl PropertyList {
    /// Source PropertyList indexer uses the first matching pair, in file order.
    pub fn first(&self, key: &str) -> Option<&str> {
        self.items
            .iter()
            .flat_map(|i| &i.pairs)
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeProperty {
    pub id: i32,
    pub properties: PropertyList,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimePropertyList {
    pub items: Vec<TimeProperty>,
}

fn property_list(reader: &mut Reader<'_>, budget: &mut DecodeBudget<'_>) -> Result<PropertyList> {
    let n = count(reader, budget, budget.limits.max_entries, "property items")?;
    budget.reserve::<PropertyItem>(
        n,
        budget.limits.max_entries,
        reader.position(),
        "property items",
    )?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let n = count(reader, budget, budget.limits.max_entries, "property pairs")?;
        budget.reserve::<(String, String)>(
            n,
            budget.limits.max_entries,
            reader.position(),
            "property pairs",
        )?;
        let mut pairs = Vec::with_capacity(n);
        for _ in 0..n {
            pairs.push((ascii(reader, budget, false)?, ascii(reader, budget, false)?));
        }
        items.push(PropertyItem { pairs });
    }
    Ok(PropertyList { items })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Motion {
    pub unknown: u32,
    pub bone_name: String,
    pub frame_count: u32,
    pub duration_ms: F32Bits,
    pub translation_flag: u8,
    pub rotation_flag: u8,
    pub first_translation_index: i32,
    pub first_rotation_index: i32,
    pub properties_flag: u8,
    pub properties: Vec<PropertyList>,
    pub time_properties_flag: u8,
    pub time_properties: Vec<TimePropertyList>,
}

impl Motion {
    pub fn has_translation(&self) -> bool {
        self.translation_flag == 1
    }
    pub fn has_rotation(&self) -> bool {
        self.rotation_flag == 1
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Animation {
    pub version: u32,
    pub name: String,
    pub duration_ms: F32Bits,
    pub distance: F32Bits,
    pub is_moving: u8,
    pub translations: Vec<Vector3Bits>,
    pub rotations: Vec<QuaternionBits>,
    pub motions: Vec<Motion>,
    pub num_frames: u32,
    pub coordinate_policy: CoordinatePolicy,
}

impl Animation {
    /// The source VM's discarded OrderBy result does not sort these properties.
    pub fn time_properties_in_source_order(&self) -> impl Iterator<Item = &TimeProperty> {
        self.motions
            .iter()
            .flat_map(|m| &m.time_properties)
            .flat_map(|p| &p.items)
    }
    /// Source UpdateFPS arithmetic and ties-to-even rounding, all in f32.
    /// Zero duration has no meaningful derived FPS and returns None.
    pub fn frames_per_second(&self) -> Option<u32> {
        if self.duration_ms.get() <= 0.0 {
            return None;
        }
        let fps =
            ((self.num_frames as f32) / (self.duration_ms.get() / 1000.0f32)).round_ties_even();
        if fps.is_finite() && (0.0..2147483648.0f32).contains(&fps) {
            Some(fps as u32)
        } else {
            None
        }
    }
}

fn index_range(first: i32, count: usize, total: usize, offset: usize, label: &str) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    if first < 0 {
        return Err(invalid(offset, label));
    }
    let end = (first as usize)
        .checked_add(count)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, label))?;
    if end > total {
        return Err(invalid(offset, label));
    }
    Ok(())
}

pub fn decode_animation(bytes: &[u8], limits: &Limits) -> Result<Animation> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 2, "unsupported standalone animation version")?;
    let name = ascii(&mut r, &mut budget, true)?;
    let duration_ms = finite_le(&mut r)?;
    if duration_ms.get() < 0.0 {
        return Err(invalid(r.position() - 4, "negative animation duration"));
    }
    let distance = finite_le(&mut r)?;
    let is_moving = r.u8()?;
    let n = count(
        &mut r,
        &mut budget,
        limits.max_frames,
        "animation translations",
    )?;
    budget.reserve::<Vector3Bits>(n, limits.max_frames, r.position(), "animation translations")?;
    let mut translations = Vec::with_capacity(n);
    for _ in 0..n {
        translations.push(vector(&mut r)?);
    }
    let n = count(
        &mut r,
        &mut budget,
        limits.max_frames,
        "animation rotations",
    )?;
    budget.reserve::<QuaternionBits>(n, limits.max_frames, r.position(), "animation rotations")?;
    let mut rotations = Vec::with_capacity(n);
    for _ in 0..n {
        rotations.push(quaternion(&mut r)?);
    }
    let n = count(&mut r, &mut budget, limits.max_entries, "animation motions")?;
    budget.reserve::<Motion>(n, limits.max_entries, r.position(), "animation motions")?;
    let mut motions = Vec::with_capacity(n);
    let mut num_frames = 0;
    for _ in 0..n {
        let unknown = r.u32_be()?;
        let bone_name = ascii(&mut r, &mut budget, false)?;
        let frame_count = count(&mut r, &mut budget, limits.max_frames, "motion frames")? as u32;
        num_frames = num_frames.max(frame_count);
        let duration_ms = finite_le(&mut r)?;
        if duration_ms.get() < 0.0 {
            return Err(invalid(r.position() - 4, "negative motion duration"));
        }
        let translation_flag = r.u8()?;
        let rotation_flag = r.u8()?;
        let first_translation_index = r.i32_be()?;
        let first_rotation_index = r.i32_be()?;
        if translation_flag == 1 {
            index_range(
                first_translation_index,
                frame_count as usize,
                translations.len(),
                r.position() - 8,
                "motion translation indices",
            )?;
        }
        if rotation_flag == 1 {
            index_range(
                first_rotation_index,
                frame_count as usize,
                rotations.len(),
                r.position() - 4,
                "motion rotation indices",
            )?;
        }
        let properties_flag = r.u8()?;
        let mut properties = Vec::new();
        if properties_flag == 1 {
            let n = count(
                &mut r,
                &mut budget,
                limits.max_entries,
                "motion property lists",
            )?;
            budget.reserve::<PropertyList>(
                n,
                limits.max_entries,
                r.position(),
                "motion property lists",
            )?;
            properties = Vec::with_capacity(n);
            for _ in 0..n {
                properties.push(property_list(&mut r, &mut budget)?);
            }
        }
        let time_properties_flag = r.u8()?;
        let mut time_properties = Vec::new();
        if time_properties_flag == 1 {
            let n = count(
                &mut r,
                &mut budget,
                limits.max_entries,
                "time property lists",
            )?;
            budget.reserve::<TimePropertyList>(
                n,
                limits.max_entries,
                r.position(),
                "time property lists",
            )?;
            time_properties = Vec::with_capacity(n);
            for _ in 0..n {
                let n = count(&mut r, &mut budget, limits.max_entries, "timed properties")?;
                budget.reserve::<TimeProperty>(
                    n,
                    limits.max_entries,
                    r.position(),
                    "timed properties",
                )?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(TimeProperty {
                        id: r.i32_be()?,
                        properties: property_list(&mut r, &mut budget)?,
                    });
                }
                time_properties.push(TimePropertyList { items });
            }
        }
        motions.push(Motion {
            unknown,
            bone_name,
            frame_count,
            duration_ms,
            translation_flag,
            rotation_flag,
            first_translation_index,
            first_rotation_index,
            properties_flag,
            properties,
            time_properties_flag,
            time_properties,
        });
    }
    finish(&r)?;
    Ok(Animation {
        version,
        name,
        duration_ms,
        distance,
        is_moving,
        translations,
        rotations,
        motions,
        num_frames,
        coordinate_policy: CoordinatePolicy::FreeSo,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bone {
    pub unknown: i32,
    pub name: String,
    pub parent_name: String,
    pub properties_flag: u8,
    pub properties: PropertyList,
    pub translation: Vector3Bits,
    pub rotation: QuaternionBits,
    pub can_translate: i32,
    pub can_rotate: i32,
    pub can_blend: i32,
    pub wiggle_value: F32Bits,
    pub wiggle_power: F32Bits,
    /// Original file index. Hierarchy resolution does not reorder the bone list.
    pub index: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skeleton {
    pub version: u32,
    pub name: String,
    pub bones: Vec<Bone>,
    pub root: usize,
    pub coordinate_policy: CoordinatePolicy,
}

impl Skeleton {
    pub fn bone(&self, name: &str) -> Option<&Bone> {
        self.bones.iter().find(|b| b.name == name)
    }
}

pub fn decode_skeleton(bytes: &[u8], limits: &Limits) -> Result<Skeleton> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 1, "unsupported standalone skeleton version")?;
    let name = ascii(&mut r, &mut budget, false)?;
    let n = r.i16_be()?;
    if n <= 0 {
        return Err(invalid(
            r.position() - 2,
            "skeleton requires a positive bone count",
        ));
    }
    let n = n as usize;
    budget.reserve::<Bone>(n, limits.max_entries, r.position() - 2, "skeleton bones")?;
    let mut bones = Vec::with_capacity(n);
    for index in 0..n {
        let unknown = r.i32_be()?;
        let name = ascii(&mut r, &mut budget, false)?;
        let parent_name = ascii(&mut r, &mut budget, false)?;
        let properties_flag = r.u8()?;
        let properties = if properties_flag != 0 {
            property_list(&mut r, &mut budget)?
        } else {
            PropertyList { items: Vec::new() }
        };
        let translation = vector(&mut r)?;
        let rotation = quaternion(&mut r)?;
        let can_translate = r.i32_be()?;
        let can_rotate = r.i32_be()?;
        let can_blend = r.i32_be()?;
        let wiggle_value = finite_le(&mut r)?;
        let wiggle_power = finite_le(&mut r)?;
        bones.push(Bone {
            unknown,
            name,
            parent_name,
            properties_flag,
            properties,
            translation,
            rotation,
            can_translate,
            can_rotate,
            can_blend,
            wiggle_value,
            wiggle_power,
            index,
            parent: None,
            children: Vec::new(),
        });
    }
    finish(&r)?;
    let root = resolve_bone_hierarchy(&mut bones, &mut budget, r.position())?;
    Ok(Skeleton {
        version,
        name,
        bones,
        root,
        coordinate_policy: CoordinatePolicy::FreeSo,
    })
}

fn resolve_bone_hierarchy(
    bones: &mut [Bone],
    budget: &mut DecodeBudget<'_>,
    offset: usize,
) -> Result<usize> {
    let n = bones.len();
    let limits = budget.limits;
    // Account for the temporary hierarchy map, parent/child edges, visit marks,
    // and iterative work list before constructing any of them.
    budget.reserve::<usize>(
        n.saturating_mul(8),
        limits.max_entries.saturating_mul(8),
        offset,
        "skeleton hierarchy scratch",
    )?;
    let mut names = BTreeMap::new();
    for bone in bones.iter() {
        if bone.name.is_empty() || bone.name == "NULL" {
            return Err(invalid(0, "invalid bone name"));
        }
        if names.insert(bone.name.as_str(), bone.index).is_some() {
            return Err(Error::new(ErrorKind::Duplicate, 0, "duplicate bone name"));
        }
    }
    let mut parents = Vec::with_capacity(n);
    let mut root = None;
    for bone in bones.iter() {
        if bone.parent_name == "NULL" {
            if root.replace(bone.index).is_some() {
                return Err(invalid(0, "multiple skeleton roots"));
            }
            parents.push(None);
        } else {
            let parent = *names
                .get(bone.parent_name.as_str())
                .ok_or_else(|| invalid(0, "missing bone parent"))?;
            parents.push(Some(parent));
        }
    }
    drop(names);
    let root = root.ok_or_else(|| invalid(0, "skeleton has no root (possibly cyclic)"))?;
    for (i, parent) in parents.into_iter().enumerate() {
        bones[i].parent = parent;
        if let Some(parent) = parent {
            bones[parent].children.push(i);
        }
    }
    let mut seen = vec![false; n];
    let mut stack = vec![(root, 1usize)];
    while let Some((i, depth)) = stack.pop() {
        limits.check_count(depth, limits.max_depth, 0, "bone hierarchy depth")?;
        if seen[i] {
            return Err(invalid(0, "cyclic bone hierarchy"));
        }
        seen[i] = true;
        for &child in bones[i].children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    if seen.iter().any(|seen| !seen) {
        return Err(invalid(0, "disconnected or cyclic bone hierarchy"));
    }
    Ok(root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceKey {
    pub group_id: u32,
    pub file_id: u32,
    pub type_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileKey {
    pub file_id: u32,
    pub type_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub version: u32,
    pub bone: String,
    pub mesh_selector: u32,
    pub mesh: Option<ResourceKey>,
    pub texture_selector: u32,
    pub texture: Option<ResourceKey>,
}

fn resource_key(r: &mut Reader<'_>, selector: u32) -> Result<Option<ResourceKey>> {
    match selector {
        0 => Ok(None),
        8 => Ok(Some(ResourceKey {
            group_id: r.u32_be()?,
            file_id: r.u32_be()?,
            type_id: r.u32_be()?,
        })),
        _ => Err(unsupported(
            r.position() - 4,
            "unsupported binding reference selector",
        )),
    }
}

pub fn decode_binding(bytes: &[u8], limits: &Limits) -> Result<Binding> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 1, "unsupported binding version")?;
    let bone = ascii(&mut r, &mut budget, false)?;
    let mesh_selector = r.u32_be()?;
    let mesh = resource_key(&mut r, mesh_selector)?;
    let texture_selector = r.u32_be()?;
    let texture = resource_key(&mut r, texture_selector)?;
    finish(&r)?;
    Ok(Binding {
        version,
        bone,
        mesh_selector,
        mesh,
        texture_selector,
        texture,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub version: u32,
    pub thumbnail: FileKey,
    pub bindings: Vec<FileKey>,
}

fn file_key(r: &mut Reader<'_>) -> Result<FileKey> {
    Ok(FileKey {
        file_id: r.u32_be()?,
        type_id: r.u32_be()?,
    })
}

pub fn decode_appearance(bytes: &[u8], limits: &Limits) -> Result<Appearance> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 1, "unsupported appearance version")?;
    let thumbnail = file_key(&mut r)?;
    let n = count(
        &mut r,
        &mut budget,
        limits.max_entries,
        "appearance bindings",
    )?;
    budget.reserve::<FileKey>(n, limits.max_entries, r.position(), "appearance bindings")?;
    let mut bindings = Vec::with_capacity(n);
    for _ in 0..n {
        bindings.push(file_key(&mut r)?);
    }
    finish(&r)?;
    Ok(Appearance {
        version,
        thumbnail,
        bindings,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outfit {
    pub version: u32,
    pub unknown: u32,
    pub light_appearance: FileKey,
    pub medium_appearance: FileKey,
    pub dark_appearance: FileKey,
    pub hand_group: u32,
    pub region: u32,
}

pub fn decode_outfit(bytes: &[u8], limits: &Limits) -> Result<Outfit> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 1, "unsupported outfit version")?;
    let result = Outfit {
        version,
        unknown: r.u32_be()?,
        light_appearance: file_key(&mut r)?,
        medium_appearance: file_key(&mut r)?,
        dark_appearance: file_key(&mut r)?,
        hand_group: r.u32_be()?,
        region: r.u32_be()?,
    };
    finish(&r)?;
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoneBinding {
    pub bone_index: i32,
    pub first_real_vertex: i32,
    pub real_vertex_count: i32,
    pub first_blend_vertex: i32,
    pub blend_vertex_count: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshVertex {
    pub texture_coordinate: [F32Bits; 2],
    pub position: Vector3Bits,
    /// Converted original bits; a zero normal is preserved here.
    pub normal: Vector3Bits,
}

impl MeshVertex {
    /// Source Mesh.Read replaces a zero real-vertex normal with (0,1,0).
    pub fn effective_normal(&self) -> Vector3Bits {
        if self.normal.iter().all(|f| f.get() == 0.0) {
            [F32Bits(0), F32Bits(0x3f80_0000), F32Bits(0)]
        } else {
            self.normal
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlendVertex {
    pub raw_weight: i32,
    pub other_vertex: i32,
    pub position: Vector3Bits,
    pub normal: Vector3Bits,
}

impl BlendVertex {
    pub fn weight(&self) -> F32Bits {
        F32Bits::from_f32(self.raw_weight as f32 / 32768.0f32)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mesh {
    pub version: u32,
    pub bone_names: Vec<String>,
    pub faces: Vec<[i32; 3]>,
    pub bindings: Vec<BoneBinding>,
    pub vertices: Vec<MeshVertex>,
    pub blend_vertices: Vec<BlendVertex>,
    pub repeated_real_vertex_count: i32,
    pub coordinate_policy: CoordinatePolicy,
}

pub fn decode_mesh(bytes: &[u8], limits: &Limits) -> Result<Mesh> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = version(&mut r, 2, "unsupported standalone mesh version")?;
    let n = signed_count(&mut r, &mut budget, limits.max_entries, "mesh bone names")?;
    budget.reserve::<String>(n, limits.max_entries, r.position(), "mesh bone names")?;
    let mut bone_names = Vec::with_capacity(n);
    for _ in 0..n {
        bone_names.push(ascii(&mut r, &mut budget, false)?);
    }
    let n = signed_count(&mut r, &mut budget, limits.max_entries, "mesh faces")?;
    budget.reserve::<[i32; 3]>(n, limits.max_entries, r.position(), "mesh faces")?;
    let mut faces = Vec::with_capacity(n);
    for _ in 0..n {
        faces.push([r.i32_be()?, r.i32_be()?, r.i32_be()?]);
    }
    let n = signed_count(&mut r, &mut budget, limits.max_entries, "mesh bindings")?;
    budget.reserve::<BoneBinding>(n, limits.max_entries, r.position(), "mesh bindings")?;
    let mut bindings = Vec::with_capacity(n);
    for _ in 0..n {
        let binding = BoneBinding {
            bone_index: r.i32_be()?,
            first_real_vertex: r.i32_be()?,
            real_vertex_count: r.i32_be()?,
            first_blend_vertex: r.i32_be()?,
            blend_vertex_count: r.i32_be()?,
        };
        if binding.bone_index < 0 || binding.bone_index as usize >= bone_names.len() {
            return Err(invalid(
                r.position() - 20,
                "mesh bone index outside bone table",
            ));
        }
        if binding.real_vertex_count < 0 || binding.blend_vertex_count < 0 {
            return Err(invalid(r.position() - 20, "negative mesh binding count"));
        }
        bindings.push(binding);
    }
    let n = signed_count(&mut r, &mut budget, limits.max_vertices, "real vertices")?;
    budget.reserve::<MeshVertex>(n, limits.max_vertices, r.position(), "real vertices")?;
    let mut vertices = Vec::with_capacity(n);
    for _ in 0..n {
        vertices.push(MeshVertex {
            texture_coordinate: [finite_le(&mut r)?, finite_le(&mut r)?],
            position: [F32Bits(0); 3],
            normal: [F32Bits(0); 3],
        });
    }
    let n = signed_count(&mut r, &mut budget, limits.max_vertices, "blend vertices")?;
    limits.check_count(
        vertices
            .len()
            .checked_add(n)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "total vertices"))?,
        limits.max_vertices,
        r.position(),
        "total vertices",
    )?;
    budget.reserve::<BlendVertex>(n, limits.max_vertices, r.position(), "blend vertices")?;
    let mut blend_vertices = Vec::with_capacity(n);
    for _ in 0..n {
        let raw_weight = r.i32_be()?;
        let other_vertex = r.i32_be()?;
        if other_vertex < 0 || other_vertex as usize >= vertices.len() {
            return Err(invalid(
                r.position() - 4,
                "blend destination outside real vertices",
            ));
        }
        blend_vertices.push(BlendVertex {
            raw_weight,
            other_vertex,
            position: [F32Bits(0); 3],
            normal: [F32Bits(0); 3],
        });
    }
    let repeated_real_vertex_count = r.i32_be()?;
    if repeated_real_vertex_count < 0 || repeated_real_vertex_count as usize != vertices.len() {
        return Err(invalid(
            r.position() - 4,
            "inconsistent repeated vertex count",
        ));
    }
    for vertex in &mut vertices {
        vertex.position = vector(&mut r)?;
        vertex.normal = vector(&mut r)?;
    }
    for vertex in &mut blend_vertices {
        vertex.position = vector(&mut r)?;
        vertex.normal = vector(&mut r)?;
    }
    finish(&r)?;
    for face in &faces {
        for &index in face {
            if index < 0 || index as usize >= vertices.len() {
                return Err(invalid(0, "face index outside real vertices"));
            }
        }
    }
    for binding in &bindings {
        index_range(
            binding.first_real_vertex,
            binding.real_vertex_count as usize,
            vertices.len(),
            0,
            "real vertex binding range",
        )?;
        index_range(
            binding.first_blend_vertex,
            binding.blend_vertex_count as usize,
            blend_vertices.len(),
            0,
            "blend vertex binding range",
        )?;
    }
    Ok(Mesh {
        version,
        bone_names,
        faces,
        bindings,
        vertices,
        blend_vertices,
        repeated_real_vertex_count,
        coordinate_policy: CoordinatePolicy::FreeSo,
    })
}
