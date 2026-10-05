// SPDX-License-Identifier: MPL-2.0
//! Bounded glTF 2.0 / GLB authoring with explicit original-resource binding.
//! Geometry attributes are editable; the protected graph, resource identities,
//! draw order and indices cannot be silently changed by an interchange tool.
use crate::json_support;
use serde_json::{json, Value};
use std::{io::Write, mem::size_of};
use wonderland_legacy_formats::{
    reconstruction::{self, FsomMesh},
    vitaboy::{self, F32Bits},
    Limits,
};

#[derive(Clone)]
pub struct GltfPackage {
    pub json: Value,
    pub binary: Vec<u8>,
}
fn integer(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| "glTF index/length must be an unsigned integer".into())
}
fn limit(bytes: usize, limits: &Limits) -> Result<(), String> {
    if bytes > limits.max_input_bytes.min(limits.max_resource_bytes)
        || bytes
            .checked_mul(16)
            .and_then(|n| n.checked_add(65536))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        Err("glTF binary working-copy limit exceeded".into())
    } else {
        Ok(())
    }
}

fn charge(total: &mut usize, count: usize, width: usize) -> Result<(), String> {
    *total = count
        .checked_mul(width)
        .and_then(|n| total.checked_add(n))
        .ok_or("glTF graph admission size overflow")?;
    Ok(())
}
fn property_retained(value: &vitaboy::PropertyList, total: &mut usize) -> Result<(), String> {
    charge(
        total,
        value.items.capacity(),
        size_of::<vitaboy::PropertyItem>(),
    )?;
    for item in &value.items {
        charge(total, item.pairs.capacity(), size_of::<(String, String)>())?;
        for (key, value) in &item.pairs {
            charge(total, key.capacity(), 1)?;
            charge(total, value.capacity(), 1)?;
        }
    }
    Ok(())
}
/// Count bounded metadata without constructing an owned JSON tree or output.
fn metadata_size<T: serde::Serialize>(value: &T, limits: &Limits) -> Result<usize, String> {
    struct Counter {
        bytes: usize,
        cap: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= self.cap)
                .ok_or_else(|| std::io::Error::other("glTF graph admission metadata limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Counter {
        bytes: 0,
        cap: json_support::MAX_EDITOR_JSON_BYTES
            .min(limits.max_input_bytes)
            .min(limits.max_resource_bytes),
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    Ok(writer.bytes)
}
fn admit_graph(retained: usize, binary: usize, json: usize, limits: &Limits) -> Result<(), String> {
    json_support::admit(json, limits).map_err(|e| format!("glTF graph admission: {e}"))?;
    limit(binary, limits)?;
    // Source/result models, attribute arrays, growable builders, serialized JSON
    // and graph comparisons coexist during export/import. Count every source
    // vector's capacity, then reserve conservative graph and binary copy bounds.
    let mut total = 65_536;
    charge(&mut total, retained, 4)?;
    charge(&mut total, binary, 16)?;
    charge(&mut total, json, 128)?;
    if total > limits.max_total_decoded_bytes {
        return Err("glTF graph admission exceeds aggregate working memory".into());
    }
    Ok(())
}
fn fsom_admission(mesh: &FsomMesh, source: &str, limits: &Limits) -> Result<(), String> {
    let mut retained = mesh.name.capacity();
    charge(
        &mut retained,
        mesh.groups.capacity(),
        size_of::<Vec<reconstruction::FsomGeometry>>(),
    )?;
    let mut binary = 0;
    let mut json = 4096;
    charge(&mut json, mesh.name.len(), 6)?;
    charge(&mut json, source.len(), 6)?;
    for group in &mesh.groups {
        charge(
            &mut retained,
            group.capacity(),
            size_of::<reconstruction::FsomGeometry>(),
        )?;
        for geometry in group {
            charge(
                &mut retained,
                geometry.vertices.capacity(),
                size_of::<reconstruction::FsomVertex>(),
            )?;
            charge(&mut retained, geometry.indices.capacity(), size_of::<i32>())?;
            charge(&mut binary, geometry.vertices.len(), 32)?;
            charge(&mut binary, geometry.indices.len(), 4)?;
            // Four accessors/views plus primitive, material, scene and node.
            // Includes maximal decimal widths for all generated indices/bounds.
            charge(&mut json, 1, 4096)?;
        }
    }
    if let Some(mask) = &mesh.depth_mask {
        charge(
            &mut retained,
            mask.vertices.capacity(),
            size_of::<reconstruction::FsomVertex>(),
        )?;
        charge(&mut retained, mask.indices.capacity(), size_of::<i32>())?;
        charge(&mut json, metadata_size(mask, limits)?, 1)?;
    }
    admit_graph(retained, binary, json, limits)
}
fn animation_admission(
    animation: &vitaboy::Animation,
    skeleton: &vitaboy::Skeleton,
    animation_hash: &str,
    skeleton_hash: &str,
    limits: &Limits,
) -> Result<(), String> {
    let mut retained = animation.name.capacity();
    charge(
        &mut retained,
        animation.translations.capacity(),
        size_of::<vitaboy::Vector3Bits>(),
    )?;
    charge(
        &mut retained,
        animation.rotations.capacity(),
        size_of::<vitaboy::QuaternionBits>(),
    )?;
    charge(
        &mut retained,
        animation.motions.capacity(),
        size_of::<vitaboy::Motion>(),
    )?;
    charge(&mut retained, skeleton.name.capacity(), 1)?;
    charge(
        &mut retained,
        skeleton.bones.capacity(),
        size_of::<vitaboy::Bone>(),
    )?;
    let mut json = 4096;
    charge(&mut json, animation_hash.len(), 6)?;
    charge(&mut json, skeleton_hash.len(), 6)?;
    charge(&mut json, animation.name.len(), 6)?;
    for bone in &skeleton.bones {
        charge(&mut retained, bone.name.capacity(), 1)?;
        charge(&mut retained, bone.parent_name.capacity(), 1)?;
        charge(&mut retained, bone.children.capacity(), size_of::<usize>())?;
        property_retained(&bone.properties, &mut retained)?;
        charge(&mut json, 1, 1024)?;
        charge(&mut json, bone.name.len(), 6)?;
        charge(&mut json, bone.children.len(), 22)?;
    }
    let mut binary = 0;
    for motion in &animation.motions {
        charge(&mut retained, motion.bone_name.capacity(), 1)?;
        charge(
            &mut retained,
            motion.properties.capacity(),
            size_of::<vitaboy::PropertyList>(),
        )?;
        for property in &motion.properties {
            property_retained(property, &mut retained)?;
        }
        charge(
            &mut retained,
            motion.time_properties.capacity(),
            size_of::<vitaboy::TimePropertyList>(),
        )?;
        for list in &motion.time_properties {
            charge(
                &mut retained,
                list.items.capacity(),
                size_of::<vitaboy::TimeProperty>(),
            )?;
            for item in &list.items {
                property_retained(&item.properties, &mut retained)?;
            }
        }
        if motion.frame_count != 0 {
            for (enabled, stride) in [(motion.has_translation(), 16), (motion.has_rotation(), 20)] {
                if enabled {
                    charge(&mut binary, motion.frame_count as usize, stride)?;
                    charge(&mut json, 1, 2048)?;
                }
            }
        }
    }
    charge(&mut json, metadata_size(&animation.motions, limits)?, 1)?;
    admit_graph(retained, binary, json, limits)
}

// Public GltfPackage fields can also be constructed without a bounded parser.
// Check their actual depth, node counts and retained capacities before cloning.
fn package_graph_memory(value: &Value, limits: &Limits) -> Result<usize, String> {
    fn visit(
        value: &Value,
        depth: usize,
        nodes: &mut usize,
        bytes: &mut usize,
        limits: &Limits,
    ) -> Result<(), String> {
        charge(nodes, 1, 1)?;
        if depth > limits.max_depth.min(32) || *nodes > limits.max_entries {
            return Err("glTF graph admission depth/node limit".into());
        }
        match value {
            Value::Array(values) => charge(bytes, values.capacity(), size_of::<Value>())?,
            Value::Object(values) => charge(bytes, values.len(), 256)?,
            Value::String(value) => charge(bytes, value.capacity(), 1)?,
            _ => (),
        }
        if *bytes > limits.max_total_decoded_bytes {
            return Err("glTF graph admission retained memory limit".into());
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value, depth + 1, nodes, bytes, limits)?;
                }
            }
            Value::Object(values) => {
                for (key, value) in values {
                    charge(bytes, key.capacity(), 1)?;
                    visit(value, depth + 1, nodes, bytes, limits)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let mut bytes = 0;
    visit(value, 0, &mut 0, &mut bytes, limits)?;
    Ok(bytes)
}

struct Builder {
    binary: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}
impl Builder {
    fn new() -> Self {
        Self {
            binary: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        }
    }
    fn f32<const N: usize>(
        &mut self,
        values: &[[f32; N]],
        bounds: bool,
        limits: &Limits,
    ) -> Result<usize, String> {
        if values.is_empty() {
            return Err("glTF accessors cannot be empty".into());
        }
        let n = values
            .len()
            .checked_mul(N * 4)
            .ok_or("glTF accessor length overflow")?;
        limit(
            self.binary
                .len()
                .checked_add(n)
                .ok_or("glTF buffer overflow")?,
            limits,
        )?;
        let offset = self.binary.len();
        let mut minimum = [f32::INFINITY; N];
        let mut maximum = [f32::NEG_INFINITY; N];
        for value in values {
            for (i, v) in value.iter().enumerate() {
                if !v.is_finite() {
                    return Err("nonfinite glTF attribute".into());
                }
                self.binary.extend_from_slice(&v.to_le_bytes());
                minimum[i] = minimum[i].min(*v);
                maximum[i] = maximum[i].max(*v);
            }
        }
        let view = self.views.len();
        self.views
            .push(json!({"buffer":0,"byteOffset":offset,"byteLength":n}));
        let kind = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            4 => "VEC4",
            16 => "MAT4",
            _ => return Err("invalid glTF accessor width".into()),
        };
        let mut accessor =
            json!({"bufferView":view,"componentType":5126,"count":values.len(),"type":kind});
        if bounds {
            accessor["min"] = json!(minimum.as_slice());
            accessor["max"] = json!(maximum.as_slice());
        }
        let id = self.accessors.len();
        self.accessors.push(accessor);
        Ok(id)
    }
    fn indices(&mut self, values: &[i32], limits: &Limits) -> Result<usize, String> {
        if values.is_empty() || !values.len().is_multiple_of(3) {
            return Err("glTF triangle index list is empty or incomplete".into());
        }
        limit(
            self.binary
                .len()
                .checked_add(values.len() * 4)
                .ok_or("glTF buffer overflow")?,
            limits,
        )?;
        let offset = self.binary.len();
        for value in values {
            let value = u32::try_from(*value).map_err(|_| "negative glTF index")?;
            self.binary.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.views.len();
        self.views
            .push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*4}));
        let id = self.accessors.len();
        self.accessors.push(
            json!({"bufferView":view,"componentType":5125,"count":values.len(),"type":"SCALAR"}),
        );
        Ok(id)
    }
    fn finish(self, mut document: Value, limits: &Limits) -> Result<GltfPackage, String> {
        document["asset"] = json!({"version":"2.0","generator":"Wonderland source-bound Creator"});
        document["buffers"] = json!([{"byteLength":self.binary.len()}]);
        document["bufferViews"] = Value::Array(self.views);
        document["accessors"] = Value::Array(self.accessors);
        package_graph_memory(&document, limits)?;
        json_support::encode(&document, limits)?;
        Ok(GltfPackage {
            json: document,
            binary: self.binary,
        })
    }
}

impl GltfPackage {
    pub fn to_glb(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        self.validate_buffer(limits)?;
        let mut json = self.json.clone();
        json["buffers"][0]
            .as_object_mut()
            .ok_or("invalid glTF buffer")?
            .remove("uri");
        let mut json = json_support::encode(&json, limits)?;
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let binary_length = (self.binary.len() + 3) & !3;
        let length = 28usize
            .checked_add(json.len())
            .and_then(|n| n.checked_add(binary_length))
            .ok_or("GLB length overflow")?;
        limit(length, limits)?;
        let length32 = u32::try_from(length).map_err(|_| "GLB too large")?;
        let mut out = Vec::with_capacity(length);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&length32.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(binary_length as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.binary);
        out.resize(length, 0);
        Ok(out)
    }
    pub fn from_glb(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        limit(bytes.len(), limits)?;
        if bytes.len() < 28
            || &bytes[..4] != b"glTF"
            || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 2
            || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize != bytes.len()
        {
            return Err("invalid GLB header/length".into());
        }
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let end = 20usize.checked_add(json_len).ok_or("GLB chunk overflow")?;
        if !json_len.is_multiple_of(4)
            || end.checked_add(8).is_none_or(|n| n > bytes.len())
            || &bytes[16..20] != b"JSON"
        {
            return Err("GLB first chunk must be aligned JSON".into());
        }
        let json = json_support::parse(&bytes[20..end], limits)?;
        let bin_len = u32::from_le_bytes(bytes[end..end + 4].try_into().unwrap()) as usize;
        if !bin_len.is_multiple_of(4)
            || end.checked_add(8).and_then(|n| n.checked_add(bin_len)) != Some(bytes.len())
            || &bytes[end + 4..end + 8] != b"BIN\0"
        {
            return Err("GLB binary extent or trailing chunk mismatch".into());
        }
        let declared = integer(&json["buffers"][0]["byteLength"])?;
        if declared > bin_len
            || bin_len - declared > 3
            || bytes[end + 8 + declared..].iter().any(|b| *b != 0)
        {
            return Err("GLB padding/declared buffer mismatch".into());
        }
        let result = Self {
            json,
            binary: bytes[end + 8..end + 8 + declared].to_vec(),
        };
        result.validate_buffer(limits)?;
        Ok(result)
    }
    pub fn to_gltf(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        self.validate_buffer(limits)?;
        let mut json = self.json.clone();
        let uri = format!(
            "data:application/octet-stream;base64,{}",
            base64(&self.binary)
        );
        json["buffers"][0]["uri"] = json!(uri);
        json_support::encode(&json, limits)
    }
    pub fn from_gltf(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        let mut json = json_support::parse(bytes, limits)?;
        let uri = json["buffers"][0]["uri"]
            .as_str()
            .ok_or("self-contained glTF buffer URI required")?;
        let encoded = uri
            .strip_prefix("data:application/octet-stream;base64,")
            .ok_or(
                "only embedded glTF buffers are admitted; use GLB for external-editor transfer",
            )?;
        let binary = unbase64(encoded, limits)?;
        json["buffers"][0]
            .as_object_mut()
            .ok_or("invalid glTF buffer")?
            .remove("uri");
        let result = Self { json, binary };
        result.validate_buffer(limits)?;
        Ok(result)
    }
    fn validate_buffer(&self, limits: &Limits) -> Result<(), String> {
        let mut total = 65_536;
        charge(&mut total, package_graph_memory(&self.json, limits)?, 8)?;
        charge(&mut total, self.binary.capacity(), 16)?;
        if total > limits.max_total_decoded_bytes {
            return Err("glTF graph admission exceeds package working memory".into());
        }
        limit(self.binary.len(), limits)?;
        if self.json["asset"]["version"] != "2.0"
            || self.json["buffers"].as_array().is_none_or(|a| a.len() != 1)
            || integer(&self.json["buffers"][0]["byteLength"])? != self.binary.len()
        {
            return Err("glTF requires version2 and one exact bounded buffer".into());
        }
        if self
            .json
            .get("extensionsRequired")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err("glTF required extensions are unsupported".into());
        }
        Ok(())
    }
    fn f32<const N: usize>(&self, id: usize, limits: &Limits) -> Result<Vec<[f32; N]>, String> {
        let accessor = self.json["accessors"]
            .get(id)
            .ok_or("glTF accessor out of range")?;
        let kind = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            4 => "VEC4",
            16 => "MAT4",
            _ => return Err("invalid accessor width".into()),
        };
        if accessor["componentType"] != 5126
            || accessor["type"] != kind
            || accessor.get("sparse").is_some()
            || accessor.get("normalized").is_some_and(|v| v != false)
        {
            return Err("glTF attribute must be dense unnormalized FLOAT".into());
        }
        let count = integer(&accessor["count"])?;
        if count == 0 || count > limits.max_vertices.max(limits.max_frames) {
            return Err("glTF accessor count limit exceeded".into());
        }
        let view = self.json["bufferViews"]
            .get(integer(&accessor["bufferView"])?)
            .ok_or("glTF view out of range")?;
        if view["buffer"] != 0 {
            return Err("glTF buffer index must be zero".into());
        }
        let view_start = view
            .get("byteOffset")
            .map(integer)
            .transpose()?
            .unwrap_or(0);
        let view_len = integer(&view["byteLength"])?;
        let accessor_offset = accessor
            .get("byteOffset")
            .map(integer)
            .transpose()?
            .unwrap_or(0);
        let stride = view
            .get("byteStride")
            .map(integer)
            .transpose()?
            .unwrap_or(N * 4);
        let used = (count - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(N * 4))
            .ok_or("glTF accessor extent overflow")?;
        if stride < N * 4
            || stride % 4 != 0
            || stride > 252
            || view_start % 4 != 0
            || accessor_offset % 4 != 0
            || accessor_offset
                .checked_add(used)
                .is_none_or(|n| n > view_len)
            || view_start
                .checked_add(view_len)
                .is_none_or(|n| n > self.binary.len())
        {
            return Err("glTF accessor exceeds aligned buffer view".into());
        }
        limit(
            count
                .checked_mul(N * 4)
                .ok_or("glTF accessor allocation overflow")?,
            limits,
        )?;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let start = view_start + accessor_offset + i * stride;
            let row = std::array::from_fn(|j| {
                f32::from_le_bytes(
                    self.binary[start + j * 4..start + j * 4 + 4]
                        .try_into()
                        .unwrap(),
                )
            });
            if row.iter().any(|v| !v.is_finite()) {
                return Err("nonfinite glTF value".into());
            }
            result.push(row);
        }
        Ok(result)
    }
    fn protect_graph(&self, baseline: &Self) -> Result<(), String> {
        for key in [
            "nodes",
            "meshes",
            "scenes",
            "scene",
            "materials",
            "skins",
            "animations",
            "extras",
        ] {
            if self.json.get(key) != baseline.json.get(key) {
                return Err(format!("source-bound glTF {key} identity/topology changed"));
            }
        }
        if self.json["accessors"].as_array().map(Vec::len)
            != baseline.json["accessors"].as_array().map(Vec::len)
        {
            return Err("source-bound glTF accessor mapping changed".into());
        }
        Ok(())
    }
    pub fn from_fsom(
        mesh: &FsomMesh,
        source_sha256: &str,
        limits: &Limits,
    ) -> Result<Self, String> {
        fsom_admission(mesh, source_sha256, limits)?;
        reconstruction::encode_fsom_payload(mesh, limits).map_err(|e| e.to_string())?;
        let mut b = Builder::new();
        let mut meshes = Vec::new();
        let mut nodes = Vec::new();
        let mut materials = Vec::new();
        for (dynamic, group) in mesh.groups.iter().enumerate() {
            for (ordinal, geom) in group.iter().enumerate() {
                if geom.vertices.is_empty() || geom.indices.is_empty() {
                    return Err(
                        "empty source FSOm geometry cannot be exported as a glTF primitive".into(),
                    );
                }
                let positions: Vec<_> = geom
                    .vertices
                    .iter()
                    .map(|v| v.position.map(F32Bits::get))
                    .collect();
                let normals: Vec<_> = geom
                    .vertices
                    .iter()
                    .map(|v| v.normal.map(F32Bits::get))
                    .collect();
                for normal in &normals {
                    unit_normal(normal)?;
                }
                let uv: Vec<_> = geom
                    .vertices
                    .iter()
                    .map(|v| v.texture_coordinate.map(F32Bits::get))
                    .collect();
                let p = b.f32(&positions, true, limits)?;
                let n = b.f32(&normals, false, limits)?;
                let t = b.f32(&uv, false, limits)?;
                let i = b.indices(&geom.indices, limits)?;
                let material = materials.len();
                materials.push(json!({"name":format!("{dynamic}:{ordinal}:{}:{}",geom.pixel_direction,geom.pixel_sprite),"pbrMetallicRoughness":{"metallicFactor":0,"roughnessFactor":1},"extras":{"pixel_direction":geom.pixel_direction,"pixel_sprite":geom.pixel_sprite}}));
                let m = meshes.len();
                meshes.push(json!({"primitives":[{"attributes":{"POSITION":p,"NORMAL":n,"TEXCOORD_0":t},"indices":i,"mode":4,"material":material}]}));
                nodes.push(json!({"name":format!("fsom:{dynamic}:{ordinal}"),"mesh":m}));
            }
        }
        if nodes.is_empty() {
            return Err("FSOm has no visible geometry".into());
        }
        b.finish(json!({"scene":0,"scenes":[{"nodes":(0..nodes.len()).collect::<Vec<_>>()}],"nodes":nodes,"meshes":meshes,"materials":materials,"extras":{"schema":"wonderland.creator.fsom-gltf.v1","source_sha256":source_sha256,"mesh_name":mesh.name,"mask_type":mesh.mask_type,"depth_mask":mesh.depth_mask,"bounds":mesh.bounds,"reconstruct_version":mesh.reconstruct_version}}),limits)
    }
    pub fn apply_fsom(
        &self,
        source: &FsomMesh,
        expected_sha256: &str,
        limits: &Limits,
    ) -> Result<FsomMesh, String> {
        self.validate_buffer(limits)?;
        let baseline = Self::from_fsom(source, expected_sha256, limits)?;
        self.protect_graph(&baseline)?;
        let mut result = source.clone();
        let mut position_changed = false;
        for (primitive, geometry) in result.groups.iter_mut().flatten().enumerate() {
            let attributes = &self.json["meshes"][primitive]["primitives"][0]["attributes"];
            let p = self.f32::<3>(integer(&attributes["POSITION"])?, limits)?;
            let n = self.f32::<3>(integer(&attributes["NORMAL"])?, limits)?;
            for normal in &n {
                unit_normal(normal)?;
            }
            let t = self.f32::<2>(integer(&attributes["TEXCOORD_0"])?, limits)?;
            if p.len() != geometry.vertices.len() || n.len() != p.len() || t.len() != p.len() {
                return Err("source-bound glTF vertex count changed".into());
            }
            // Indices and all nonattribute bytes are compared after zeroing editable spans below.
            for (index, vertex) in geometry.vertices.iter_mut().enumerate() {
                let position = p[index].map(F32Bits::from_f32);
                position_changed |= position != vertex.position;
                vertex.position = position;
                vertex.normal = n[index].map(F32Bits::from_f32);
                vertex.texture_coordinate = t[index].map(F32Bits::from_f32);
            }
        }
        // Require the same dense layout so no changed index bytes or hidden buffer
        // semantics can be silently accepted. min/max may be regenerated by editors.
        let mut a = self.json.clone();
        let mut b = baseline.json.clone();
        for json in [&mut a, &mut b] {
            if let Some(accessors) = json["accessors"].as_array_mut() {
                for accessor in accessors {
                    accessor
                        .as_object_mut()
                        .ok_or("invalid accessor")?
                        .remove("min");
                    accessor.as_object_mut().unwrap().remove("max");
                }
            }
            json["asset"] = Value::Null;
        }
        if a != b {
            return Err("source-bound glTF buffer layout changed".into());
        }
        let mut expected = baseline.binary.clone();
        for mesh in self.json["meshes"].as_array().unwrap() {
            for id in mesh["primitives"][0]["attributes"]
                .as_object()
                .unwrap()
                .values()
            {
                let accessor = &self.json["accessors"][integer(id)?];
                let view = &self.json["bufferViews"][integer(&accessor["bufferView"])?];
                let offset = integer(&view["byteOffset"])?;
                let count = integer(&view["byteLength"])?;
                expected[offset..offset + count]
                    .copy_from_slice(&self.binary[offset..offset + count]);
            }
        }
        if expected != self.binary {
            return Err("source-bound glTF immutable index data changed".into());
        }
        if position_changed {
            let points = result
                .groups
                .iter()
                .flatten()
                .flat_map(|g| &g.vertices)
                .chain(
                    result
                        .depth_mask
                        .iter()
                        .filter(|_| result.mask_type != 2)
                        .flat_map(|g| &g.vertices),
                );
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for v in points {
                for i in 0..3 {
                    min[i] = min[i].min(v.position[i].get());
                    max[i] = max[i].max(v.position[i].get());
                }
            }
            result.bounds = [min.map(F32Bits::from_f32), max.map(F32Bits::from_f32)];
        }
        reconstruction::encode_fsom_payload(&result, limits).map_err(|e| e.to_string())?;
        Ok(result)
    }
    /// A real skeleton and animation are required. The source exporter uses
    /// 36 frames/sec, (-Z,-X,Y)/3 local positions and ROOT's extra quarter turns.
    /// Unit node scale retains the same world transforms without editor-only
    /// visualization bone-length scales. Time-property lists stay in source order.
    pub fn from_animation(
        animation: &vitaboy::Animation,
        skeleton: &vitaboy::Skeleton,
        animation_sha256: &str,
        skeleton_sha256: &str,
        limits: &Limits,
    ) -> Result<Self, String> {
        animation_admission(
            animation,
            skeleton,
            animation_sha256,
            skeleton_sha256,
            limits,
        )?;
        vitaboy::encode_animation(animation, limits).map_err(|e| e.to_string())?;
        vitaboy::encode_skeleton(skeleton, limits).map_err(|e| e.to_string())?;
        let mut nodes = Vec::new();
        for bone in &skeleton.bones {
            let root = bone.name == "ROOT";
            quaternion(&export_rotation(bone.rotation.map(F32Bits::get), root))?;
            let mut node = json!({"name":bone.name,"translation":export_translation(bone.translation.map(F32Bits::get),root),"rotation":export_rotation(bone.rotation.map(F32Bits::get),root)});
            if !bone.children.is_empty() {
                node["children"] = json!(bone.children);
            }
            nodes.push(node);
        }
        let mut b = Builder::new();
        let mut channels = Vec::new();
        let mut samplers = Vec::new();
        let mut targets = std::collections::BTreeSet::new();
        for (motion_index, motion) in animation.motions.iter().enumerate() {
            if motion.frame_count == 0 {
                continue;
            }
            let bone = skeleton
                .bones
                .iter()
                .position(|b| b.name == motion.bone_name)
                .ok_or("animation motion bone missing from supplied skeleton")?;
            let root = motion.bone_name == "ROOT";
            let times: Vec<_> = (0..motion.frame_count)
                .map(|frame| [frame as f32 * (1.0f32 / 36.0)])
                .collect();
            for (enabled, path) in [
                (motion.has_translation(), "translation"),
                (motion.has_rotation(), "rotation"),
            ] {
                if !enabled {
                    continue;
                }
                if !targets.insert((bone, path)) {
                    return Err(
                        "multiple source motions target the same glTF node transform".into(),
                    );
                }
                let input = b.f32(&times, true, limits)?;
                let output = if path == "translation" {
                    let values: Vec<_> = (0..motion.frame_count as usize)
                        .map(|frame| {
                            export_translation(
                                animation.translations
                                    [motion.first_translation_index as usize + frame]
                                    .map(F32Bits::get),
                                root,
                            )
                        })
                        .collect();
                    b.f32(&values, false, limits)?
                } else {
                    let values: Vec<_> = (0..motion.frame_count as usize)
                        .map(|frame| {
                            export_rotation(
                                animation.rotations[motion.first_rotation_index as usize + frame]
                                    .map(F32Bits::get),
                                root,
                            )
                        })
                        .collect();
                    for q in &values {
                        quaternion(q)?;
                    }
                    b.f32(&values, false, limits)?
                };
                let sampler = samplers.len();
                samplers.push(json!({"input":input,"output":output,"interpolation":"LINEAR"}));
                channels.push(json!({"sampler":sampler,"target":{"node":bone,"path":path},"extras":{"source_motion":motion_index}}));
            }
        }
        if channels.is_empty() {
            return Err("source animation has no nonempty transform channels".into());
        }
        b.finish(json!({"scene":0,"scenes":[{"nodes":[skeleton.root]}],"nodes":nodes,"animations":[{"name":animation.name,"samplers":samplers,"channels":channels}],"extras":{"schema":"wonderland.creator.animation-gltf.v1","animation_sha256":animation_sha256,"skeleton_sha256":skeleton_sha256,"source_fps":36,"motions":animation.motions}}),limits)
    }
    pub fn apply_animation(
        &self,
        source: &vitaboy::Animation,
        skeleton: &vitaboy::Skeleton,
        animation_sha256: &str,
        skeleton_sha256: &str,
        limits: &Limits,
    ) -> Result<vitaboy::Animation, String> {
        self.validate_buffer(limits)?;
        let baseline =
            Self::from_animation(source, skeleton, animation_sha256, skeleton_sha256, limits)?;
        self.protect_graph(&baseline)?;
        // A source-bound authoring exchange protects graph/layout and event metadata;
        // binary transform values are the only editable fields in this operation.
        let mut graph = self.json.clone();
        let mut original = baseline.json.clone();
        graph["asset"] = Value::Null;
        original["asset"] = Value::Null;
        if graph != original {
            return Err("source-bound animation glTF layout changed".into());
        }
        let mut result = source.clone();
        let mut translation_seen = vec![false; source.translations.len()];
        let mut rotation_seen = vec![false; source.rotations.len()];
        let mut accepted = baseline.binary.clone();
        for channel in self.json["animations"][0]["channels"]
            .as_array()
            .ok_or("animation channels missing")?
        {
            let motion_index = integer(&channel["extras"]["source_motion"])?;
            let motion = &source.motions[motion_index];
            let root = motion.bone_name == "ROOT";
            let sampler = &self.json["animations"][0]["samplers"][integer(&channel["sampler"])?];
            let output = integer(&sampler["output"])?;
            if channel["target"]["path"] == "translation" {
                let values = self.f32::<3>(output, limits)?;
                let originals = baseline.f32::<3>(output, limits)?;
                if values.len() != motion.frame_count as usize {
                    return Err("glTF motion frame count changed".into());
                }
                for (frame, (value, old)) in values.iter().zip(originals).enumerate() {
                    let index = motion.first_translation_index as usize + frame;
                    let candidate = if value.map(f32::to_bits) == old.map(f32::to_bits) {
                        source.translations[index]
                    } else {
                        let candidate = import_translation(*value, root);
                        if export_translation(candidate, root).map(f32::to_bits)
                            != value.map(f32::to_bits)
                        {
                            return Err(
                                "glTF translation edit does not round-trip in source binary32"
                                    .into(),
                            );
                        }
                        candidate.map(F32Bits::from_f32)
                    };
                    if translation_seen[index] && result.translations[index] != candidate {
                        return Err(
                            "glTF channels conflict on a shared source translation sample".into(),
                        );
                    }
                    result.translations[index] = candidate;
                    translation_seen[index] = true;
                }
            } else {
                let values = self.f32::<4>(output, limits)?;
                let originals = baseline.f32::<4>(output, limits)?;
                if values.len() != motion.frame_count as usize {
                    return Err("glTF rotation frame count changed".into());
                }
                for (frame, (value, old)) in values.iter().zip(originals).enumerate() {
                    quaternion(value)?;
                    let index = motion.first_rotation_index as usize + frame;
                    let candidate = if value.map(f32::to_bits) == old.map(f32::to_bits) {
                        source.rotations[index]
                    } else {
                        let candidate = import_rotation(*value, root);
                        if export_rotation(candidate, root).map(f32::to_bits)
                            != value.map(f32::to_bits)
                        {
                            return Err(
                                "glTF rotation edit does not round-trip in source binary32".into(),
                            );
                        }
                        candidate.map(F32Bits::from_f32)
                    };
                    if rotation_seen[index] && result.rotations[index] != candidate {
                        return Err(
                            "glTF channels conflict on a shared source rotation sample".into()
                        );
                    }
                    result.rotations[index] = candidate;
                    rotation_seen[index] = true;
                }
            }
            let accessor = &self.json["accessors"][output];
            let view = &self.json["bufferViews"][integer(&accessor["bufferView"])?];
            let offset = integer(&view["byteOffset"])?;
            let length = integer(&view["byteLength"])?;
            accepted[offset..offset + length]
                .copy_from_slice(&self.binary[offset..offset + length]);
        }
        if accepted != self.binary {
            return Err("source-bound animation timestamps or unbound binary data changed".into());
        }
        vitaboy::encode_animation(&result, limits).map_err(|e| e.to_string())?;
        Ok(result)
    }
}

fn export_translation(p: [f32; 3], root: bool) -> [f32; 3] {
    let scale = 1.0f32 / 3.0;
    if root {
        [-p[0] * scale, p[1] * scale, -p[2] * scale]
    } else {
        [-p[2] * scale, -p[0] * scale, p[1] * scale]
    }
}
fn import_translation(p: [f32; 3], root: bool) -> [f32; 3] {
    if root {
        [-p[0] * 3.0, p[1] * 3.0, -p[2] * 3.0]
    } else {
        [-p[1] * 3.0, p[2] * 3.0, -p[0] * 3.0]
    }
}
fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
fn export_rotation(q: [f32; 4], root: bool) -> [f32; 4] {
    let q = [-q[2], -q[0], q[1], q[3]];
    if root {
        multiply([-0.5, -0.5, -0.5, 0.5], q)
    } else {
        q
    }
}
fn import_rotation(q: [f32; 4], root: bool) -> [f32; 4] {
    let q = if root {
        multiply([0.5, 0.5, 0.5, 0.5], q)
    } else {
        q
    };
    [-q[1], q[2], -q[0], q[3]]
}
fn quaternion(q: &[f32; 4]) -> Result<(), String> {
    let length = q.iter().map(|v| v * v).sum::<f32>();
    if !length.is_finite() || (length - 1.0).abs() > 0.002 {
        Err("glTF rotations require unit source quaternions".into())
    } else {
        Ok(())
    }
}
fn unit_normal(normal: &[f32; 3]) -> Result<(), String> {
    let length = normal.iter().map(|v| v * v).sum::<f32>();
    if !length.is_finite() || (length - 1.0).abs() > 0.002 {
        Err("glTF NORMAL requires an authored unit normal; source bits are not normalized implicitly".into())
    } else {
        Ok(())
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(a >> 2) as usize] as char);
        out.push(ALPHABET[((a & 3) << 4 | b >> 4) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((b & 15) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn unbase64(text: &str, limits: &Limits) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(4) {
        return Err("invalid base64 length".into());
    }
    limit(text.len() / 4 * 3, limits)?;
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks_exact(4) {
        let mut v = [0u8; 4];
        for i in 0..4 {
            v[i] = match chunk[i] {
                b'A'..=b'Z' => chunk[i] - b'A',
                b'a'..=b'z' => chunk[i] - b'a' + 26,
                b'0'..=b'9' => chunk[i] - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => 0,
                _ => return Err("invalid base64 alphabet".into()),
            };
        }
        out.push(v[0] << 2 | v[1] >> 4);
        if chunk[2] != b'=' {
            out.push(v[1] << 4 | v[2] >> 2);
        }
        if chunk[3] != b'=' {
            out.push(v[2] << 6 | v[3]);
        }
    }
    if base64(&out) != text {
        return Err("noncanonical base64 padding".into());
    }
    Ok(out)
}
