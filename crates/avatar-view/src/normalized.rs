//! B-compatible normalized DTOs. Binary decoding and resource precedence belong to B.
use serde::{Deserialize, Serialize};
use wonderland_render_core::math::{Quat, Vec3};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct F32Bits(pub u32);
impl F32Bits {
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
    pub fn from_f32(v: f32) -> Self {
        Self(v.to_bits())
    }
}
pub type Vector3Bits = [F32Bits; 3];
pub type QuaternionBits = [F32Bits; 4];
pub fn bits3(v: Vec3) -> Vector3Bits {
    [
        F32Bits::from_f32(v.x),
        F32Bits::from_f32(v.y),
        F32Bits::from_f32(v.z),
    ]
}
pub fn bits4(q: Quat) -> QuaternionBits {
    [
        F32Bits::from_f32(q.x),
        F32Bits::from_f32(q.y),
        F32Bits::from_f32(q.z),
        F32Bits::from_f32(q.w),
    ]
}
pub(crate) fn vec3(v: Vector3Bits) -> Vec3 {
    Vec3::new(v[0].get(), v[1].get(), v[2].get())
}
pub(crate) fn quat(v: QuaternionBits) -> Quat {
    Quat::new(v[0].get(), v[1].get(), v[2].get(), v[3].get())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinatePolicy {
    Source,
    FreeSo,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyItem {
    pub pairs: Vec<(String, String)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyList {
    pub items: Vec<PropertyItem>,
}
impl PropertyList {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoneBinding {
    pub bone_index: i32,
    pub first_real_vertex: i32,
    pub real_vertex_count: i32,
    pub first_blend_vertex: i32,
    pub blend_vertex_count: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceVertex {
    pub texture_coordinate: [F32Bits; 2],
    pub position: Vector3Bits,
    pub normal: Vector3Bits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlendVertex {
    pub raw_weight: i32,
    pub other_vertex: i32,
    pub position: Vector3Bits,
    pub normal: Vector3Bits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMesh {
    pub version: u32,
    pub bone_names: Vec<String>,
    pub faces: Vec<[i32; 3]>,
    pub bindings: Vec<BoneBinding>,
    pub vertices: Vec<SourceVertex>,
    pub blend_vertices: Vec<BlendVertex>,
    pub repeated_real_vertex_count: i32,
    pub coordinate_policy: CoordinatePolicy,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceKey {
    pub group_id: u32,
    pub file_id: u32,
    pub type_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileKey {
    pub file_id: u32,
    pub type_id: u32,
}
impl FileKey {
    pub fn packed(self) -> u64 {
        ((self.file_id as u64) << 32) | self.type_id as u64
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub version: u32,
    pub thumbnail: FileKey,
    pub bindings: Vec<FileKey>,
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
