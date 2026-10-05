// SPDX-License-Identifier: MPL-2.0
//! Source OBJ/MTL interchange for Volcanic FSOm mesh overrides.
//! Source: tso.files/RC/{OBJReader,DGRP3DMesh,DGRP3DGeometry}.cs.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use wonderland_legacy_formats::reconstruction::{self, FsomGeometry, FsomMesh, FsomVertex};
use wonderland_legacy_formats::{vitaboy::F32Bits, Limits};

#[derive(Clone, Debug, PartialEq)]
pub struct ObjCorner {
    pub position: usize,
    pub texture: usize,
    pub normal: Option<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObjModel {
    pub positions: Vec<[f32; 3]>,
    pub textures: Vec<[f32; 2]>,
    pub normals: Vec<[f32; 3]>,
    pub groups: BTreeMap<String, Vec<[ObjCorner; 3]>>,
    original: Vec<u8>,
}
fn admit(bytes: usize, limits: &Limits) -> Result<(), String> {
    if bytes > limits.max_input_bytes.min(limits.max_resource_bytes)
        || bytes
            .checked_mul(64)
            .and_then(|n| n.checked_add(65536))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        return Err("OBJ/MTL working-copy limit exceeded".into());
    }
    Ok(())
}
fn float(value: &str) -> Result<f32, String> {
    let f = value.parse::<f32>().map_err(|_| "invalid OBJ/MTL float")?;
    if !f.is_finite() {
        Err("nonfinite OBJ/MTL float".into())
    } else {
        Ok(f)
    }
}
fn name(value: &str, limits: &Limits) -> Result<(), String> {
    if value.is_empty()
        || value.len() > limits.max_string_bytes
        || value.bytes().any(|b| {
            b.is_ascii_control() || b.is_ascii_whitespace() || b == b'#' || b == b'/' || b == b'\\'
        })
    {
        Err("invalid OBJ/MTL name".into())
    } else {
        Ok(())
    }
}
fn index(value: &str, len: usize) -> Result<usize, String> {
    let n = value
        .parse::<usize>()
        .map_err(|_| "OBJ indices must be positive")?;
    if n == 0 || n > len {
        Err("OBJ index out of range".into())
    } else {
        Ok(n - 1)
    }
}
#[derive(Clone, Copy)]
struct SourceGroup {
    dynamic: Option<usize>,
    sprite: u16,
    direction: u16,
    mask_type: i32,
}
fn source_group(group: &str, limits: &Limits) -> Result<SourceGroup, String> {
    let mut parts = group.split('_');
    let identity = match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("DEPTH"), Some("MASK"), None, None, None) => SourceGroup {
            dynamic: None,
            sprite: 0,
            direction: 0,
            mask_type: 1,
        },
        (Some("DEPTH"), Some("MASK"), Some("PORTAL"), None, None) => SourceGroup {
            dynamic: None,
            sprite: 0,
            direction: 0,
            mask_type: 2,
        },
        (Some(dynamic), Some("TEX"), Some(sprite), None, None) => SourceGroup {
            dynamic: Some(dynamic.parse().map_err(|_| "invalid OBJ dynamic group")?),
            sprite: sprite.parse().map_err(|_| "invalid custom texture ID")?,
            direction: u16::MAX,
            mask_type: 0,
        },
        (Some(dynamic), Some("SPR"), Some(rotation), Some(sprite), None) => SourceGroup {
            dynamic: Some(dynamic.parse().map_err(|_| "invalid OBJ dynamic group")?),
            sprite: sprite.parse().map_err(|_| "invalid sprite index")?,
            direction: rotation
                .strip_prefix("rot")
                .ok_or("sprite rotation requires rot prefix")?
                .parse()
                .map_err(|_| "invalid sprite rotation")?,
            mask_type: 0,
        },
        _ => {
            return Err(
                "OBJ object name must identify source sprite, custom texture or depth mask".into(),
            )
        }
    };
    if identity.dynamic.is_some_and(|id| id >= limits.max_entries) {
        return Err("OBJ dynamic group limit exceeded".into());
    }
    Ok(identity)
}
struct TextOutput {
    text: Option<String>,
    length: usize,
    limit: usize,
}
impl std::fmt::Write for TextOutput {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.length = self.length.checked_add(text.len()).ok_or(std::fmt::Error)?;
        if self.length > self.limit {
            return Err(std::fmt::Error);
        }
        if let Some(output) = &mut self.text {
            output.push_str(text);
        }
        Ok(())
    }
}
fn text_output(
    write: impl Fn(&mut TextOutput) -> std::fmt::Result,
    limits: &Limits,
) -> Result<Vec<u8>, String> {
    let cap = limits
        .max_input_bytes
        .min(limits.max_resource_bytes)
        .min(limits.max_total_decoded_bytes.saturating_sub(65536) / 64);
    let mut count = TextOutput {
        text: None,
        length: 0,
        limit: cap,
    };
    write(&mut count).map_err(|_| "OBJ/MTL encoded output limit exceeded")?;
    let mut output = TextOutput {
        text: Some(String::with_capacity(count.length)),
        length: 0,
        limit: count.length,
    };
    write(&mut output).map_err(|_| "OBJ/MTL encoded output changed during serialization")?;
    Ok(output.text.unwrap().into_bytes())
}
impl ObjModel {
    pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        admit(bytes.len(), limits)?;
        let text = std::str::from_utf8(bytes).map_err(|_| "OBJ must be UTF-8")?;
        let mut model = Self {
            positions: Vec::new(),
            textures: Vec::new(),
            normals: Vec::new(),
            groups: BTreeMap::new(),
            original: bytes.to_vec(),
        };
        let mut group = "_default".to_string();
        model.groups.insert(group.clone(), Vec::new());
        let mut faces = 0usize;
        for line in text.lines() {
            if line.len() > limits.max_string_bytes {
                return Err("OBJ line limit exceeded".into());
            }
            let fields: Vec<_> = line
                .split('#')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect();
            if fields.is_empty() {
                continue;
            }
            match fields[0] {
                "o" => {
                    if fields.len() != 2 {
                        return Err("OBJ object needs one name".into());
                    }
                    name(fields[1], limits)?;
                    group = fields[1].into();
                    model.groups.entry(group.clone()).or_default();
                }
                "v" | "vn" => {
                    if fields.len() != 4 {
                        return Err("OBJ vector needs three coordinates".into());
                    }
                    let values = [float(fields[1])?, float(fields[2])?, float(fields[3])?];
                    if fields[0] == "v" {
                        model.positions.push(values);
                    } else {
                        model.normals.push(values);
                    }
                }
                "vt" => {
                    if fields.len() != 3 {
                        return Err("OBJ texture coordinate needs two values".into());
                    }
                    model.textures.push([float(fields[1])?, float(fields[2])?]);
                }
                "f" => {
                    if fields.len() != 4 {
                        return Err("source OBJ authoring requires triangulated faces".into());
                    }
                    let mut corners = Vec::new();
                    for field in &fields[1..] {
                        let values: Vec<_> = field.split('/').collect();
                        if !(2..=3).contains(&values.len()) {
                            return Err("OBJ faces require position/texture[/normal]".into());
                        }
                        corners.push(ObjCorner {
                            position: index(values[0], model.positions.len())?,
                            texture: index(values[1], model.textures.len())?,
                            normal: if values.len() == 3 {
                                Some(index(values[2], model.normals.len())?)
                            } else {
                                None
                            },
                        });
                    }
                    model
                        .groups
                        .get_mut(&group)
                        .unwrap()
                        .push(corners.try_into().unwrap());
                    faces += 1;
                }
                "mtllib" | "usemtl" => {
                    if fields.len() != 2 {
                        return Err("OBJ material directive needs one relative name".into());
                    }
                    name(fields[1], limits)?;
                }
                "s" | "g" => {} // Original OBJReader ignores these directives.
                _ => return Err(format!("unsupported OBJ directive {}", fields[0])),
            }
            if model.positions.len() > limits.max_vertices
                || model.normals.len() > limits.max_vertices
                || model.textures.len() > limits.max_vertices
                || faces > limits.max_entries
                || model.groups.len() > limits.max_entries
            {
                return Err("OBJ entry limit exceeded".into());
            }
        }
        Ok(model)
    }
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }
    pub fn set_position(&mut self, index: usize, position: [f32; 3]) -> Result<(), String> {
        if position.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite OBJ position".into());
        }
        *self
            .positions
            .get_mut(index)
            .ok_or("OBJ vertex index out of range")? = position;
        Ok(())
    }
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        let budget = self.output_limits(limits)?;
        text_output(
            |out| {
                for p in &self.positions {
                    writeln!(out, "v {} {} {}", p[0], p[1], p[2])?;
                }
                for p in &self.textures {
                    writeln!(out, "vt {} {}", p[0], p[1])?;
                }
                for p in &self.normals {
                    writeln!(out, "vn {} {} {}", p[0], p[1], p[2])?;
                }
                for (group, triangles) in &self.groups {
                    if triangles.is_empty() {
                        continue;
                    }
                    writeln!(out, "o {group}\nusemtl {group}")?;
                    for triangle in triangles {
                        out.write_char('f')?;
                        for corner in triangle {
                            write!(out, " {}/{}", corner.position + 1, corner.texture + 1)?;
                            if let Some(normal) = corner.normal {
                                write!(out, "/{}", normal + 1)?;
                            }
                        }
                        out.write_char('\n')?;
                    }
                }
                Ok(())
            },
            &budget,
        )
    }
    pub fn source_materials(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        let budget = self.output_limits(limits)?;
        text_output(
            |out| {
                for group in self.groups.keys().filter(|g| g.as_str() != "_default") {
                    writeln!(out,"newmtl {group}\nKa 1 1 1\nKd 1 1 1\nKs 0 0 0\nNs 10\nillum 2\nmap_Kd {group}.png\nmap_d {group}.png")?;
                }
                Ok(())
            },
            &budget,
        )
    }
    fn output_limits(&self, limits: &Limits) -> Result<Limits, String> {
        if self.positions.len() > limits.max_vertices
            || self.textures.len() > limits.max_vertices
            || self.normals.len() > limits.max_vertices
            || self.groups.len() > limits.max_entries
        {
            return Err("OBJ model entry limit exceeded".into());
        }
        if self
            .positions
            .iter()
            .flatten()
            .chain(self.normals.iter().flatten())
            .chain(self.textures.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite OBJ attribute".into());
        }
        let mut retained = self.original.capacity();
        let mut faces = 0usize;
        let mut add = |count: usize, width: usize| -> Result<(), String> {
            retained = retained
                .checked_add(
                    count
                        .checked_mul(width)
                        .ok_or("OBJ retained allocation overflow")?,
                )
                .ok_or("OBJ retained allocation overflow")?;
            Ok(())
        };
        add(self.positions.capacity(), std::mem::size_of::<[f32; 3]>())?;
        add(self.textures.capacity(), std::mem::size_of::<[f32; 2]>())?;
        add(self.normals.capacity(), std::mem::size_of::<[f32; 3]>())?;
        add(self.groups.len(), 256)?;
        for (group, triangles) in &self.groups {
            name(group, limits)?;
            add(group.capacity(), 1)?;
            add(triangles.capacity(), std::mem::size_of::<[ObjCorner; 3]>())?;
            faces = faces
                .checked_add(triangles.len())
                .ok_or("OBJ face count overflow")?;
            if faces > limits.max_entries {
                return Err("OBJ model face limit exceeded".into());
            }
            for corner in triangles.iter().flatten() {
                if corner.position >= self.positions.len()
                    || corner.texture >= self.textures.len()
                    || corner.normal.is_some_and(|n| n >= self.normals.len())
                {
                    return Err("OBJ model corner reference out of range".into());
                }
            }
        }
        let mut result = *limits;
        result.max_total_decoded_bytes = limits
            .max_total_decoded_bytes
            .checked_sub(retained)
            .ok_or("OBJ retained allocation limit exceeded")?;
        Ok(result)
    }
    pub fn to_fsom(&self, mesh_name: &str, limits: &Limits) -> Result<FsomMesh, String> {
        let remaining = self.output_limits(limits)?;
        name(mesh_name, limits)?;
        if mesh_name.len() > 255 || !mesh_name.is_ascii() {
            return Err("source FSOm name must fit an ASCII Pascal string".into());
        }
        let mut dense_slots = 0usize;
        let mut corners = 0usize;
        let mut geometries = 0usize;
        let mut masks = 0usize;
        for (group, triangles) in &self.groups {
            if group == "_default" {
                continue;
            }
            let identity = source_group(group, limits)?;
            if let Some(id) = identity.dynamic {
                dense_slots =
                    dense_slots.max(id.checked_add(1).ok_or("OBJ dynamic group overflow")?);
            } else {
                masks += 1;
                if masks > 1 {
                    return Err("multiple source depth masks are ambiguous".into());
                }
            }
            geometries = geometries
                .checked_add(1)
                .ok_or("OBJ geometry count overflow")?;
            corners = triangles
                .len()
                .checked_mul(3)
                .and_then(|n| corners.checked_add(n))
                .ok_or("OBJ corner count overflow")?;
        }
        // Dense slots follow the largest dynamic ID, not the number of named
        // groups. Admit that whole table before resize. Each corner allowance
        // covers growable vertices/indices, remap nodes, normals, bounds scratch
        // and encoded output; `remaining` already subtracts the entire OBJ.
        let total = dense_slots
            .checked_mul(std::mem::size_of::<Vec<FsomGeometry>>())
            .and_then(|n| {
                geometries
                    .checked_mul(4 * std::mem::size_of::<FsomGeometry>())
                    .and_then(|g| n.checked_add(g))
            })
            .and_then(|n| corners.checked_mul(512).and_then(|c| n.checked_add(c)))
            .and_then(|n| dense_slots.checked_mul(4).and_then(|d| n.checked_add(d)))
            .and_then(|n| n.checked_add(65_536 + mesh_name.len()))
            .ok_or("OBJ conversion admission overflow")?;
        if total > remaining.max_total_decoded_bytes {
            return Err("OBJ conversion admission exceeds aggregate working memory".into());
        }
        let mut groups: Vec<Vec<FsomGeometry>> = Vec::with_capacity(dense_slots);
        groups.resize_with(dense_slots, Vec::new);
        let mut mask = None;
        let mut mask_type = 0;
        let mut portal_points = Vec::new();
        for (group, triangles) in &self.groups {
            if group == "_default" {
                continue;
            }
            let identity = source_group(group, limits)?;
            let mut vertices = Vec::new();
            let mut indices = Vec::new();
            let mut remap = BTreeMap::new();
            let mut any_normals = false;
            for corner in triangles.iter().flatten() {
                let key = (corner.position, corner.texture, corner.normal);
                let index = if let Some(index) = remap.get(&key) {
                    *index
                } else {
                    let position = *self
                        .positions
                        .get(corner.position)
                        .ok_or("OBJ position out of range")?;
                    let uv = *self
                        .textures
                        .get(corner.texture)
                        .ok_or("OBJ UV out of range")?;
                    let normal = if let Some(n) = corner.normal {
                        any_normals = true;
                        *self.normals.get(n).ok_or("OBJ normal out of range")?
                    } else {
                        [0.0; 3]
                    };
                    let index =
                        i32::try_from(vertices.len()).map_err(|_| "OBJ vertex index overflow")?;
                    vertices.push(FsomVertex {
                        position: bits(position),
                        texture_coordinate: [
                            F32Bits::from_f32(uv[0]),
                            F32Bits::from_f32(1.0 - uv[1]),
                        ],
                        normal: bits(normal),
                    });
                    remap.insert(key, index);
                    index
                };
                indices.push(index);
                if !group.starts_with("DEPTH_MASK_PORTAL") {
                    portal_points.push(self.positions[corner.position]);
                }
            }
            if !any_normals {
                generate_normals(&mut vertices, &indices)?;
            }
            let geom = FsomGeometry {
                pixel_sprite: identity.sprite,
                pixel_direction: identity.direction,
                vertices,
                indices,
            };
            if identity.mask_type != 0 {
                mask_type = identity.mask_type;
                if mask.replace(geom).is_some() {
                    return Err("multiple source depth masks are ambiguous".into());
                }
            } else {
                let id = identity.dynamic.unwrap();
                groups[id].push(geom);
            }
        }
        let points = if mask_type == 2 {
            portal_points.as_slice()
        } else {
            self.positions.as_slice()
        };
        let bounds = bounds(points)?;
        let result = FsomMesh {
            version: 3,
            reconstruct_version: 0,
            name: mesh_name.into(),
            groups,
            mask_type,
            depth_mask: mask,
            bounds,
        };
        reconstruction::encode_fsom_payload(&result, limits).map_err(|e| e.to_string())?;
        Ok(result)
    }
    pub fn from_fsom(mesh: &FsomMesh, limits: &Limits) -> Result<Self, String> {
        reconstruction::encode_fsom_payload(mesh, limits).map_err(|e| e.to_string())?;
        let mut retained = mesh.name.capacity();
        let mut add_size = |count: usize, width: usize| -> Result<(), String> {
            retained = count
                .checked_mul(width)
                .and_then(|n| retained.checked_add(n))
                .ok_or("FSOm to OBJ conversion admission overflow")?;
            Ok(())
        };
        add_size(
            mesh.groups.capacity(),
            std::mem::size_of::<Vec<FsomGeometry>>(),
        )?;
        for group in &mesh.groups {
            add_size(group.capacity(), std::mem::size_of::<FsomGeometry>())?;
        }
        let geometries = || mesh.groups.iter().flatten().chain(mesh.depth_mask.iter());
        let mut output = 0usize;
        for geometry in geometries() {
            add_size(
                geometry.vertices.capacity(),
                std::mem::size_of::<FsomVertex>(),
            )?;
            add_size(geometry.indices.capacity(), std::mem::size_of::<i32>())?;
            // Expanded OBJ corners are much larger than source i32 indices.
            // Account Vec growth, position/UV/normal arrays and map/name nodes.
            output = geometry
                .vertices
                .len()
                .checked_mul(64)
                .and_then(|n| {
                    geometry
                        .indices
                        .len()
                        .checked_mul(2 * std::mem::size_of::<ObjCorner>())
                        .and_then(|i| n.checked_add(i))
                })
                .and_then(|n| n.checked_add(512))
                .and_then(|n| output.checked_add(n))
                .ok_or("FSOm to OBJ conversion admission overflow")?;
        }
        let available = limits
            .max_total_decoded_bytes
            .checked_sub(retained)
            .and_then(|n| n.checked_sub(output))
            .and_then(|n| n.checked_sub(65_536))
            .ok_or("FSOm to OBJ conversion admission exceeds aggregate working memory")?;
        let mut text = TextOutput {
            text: None,
            length: 0,
            limit: (available / 64)
                .min(limits.max_input_bytes)
                .min(limits.max_resource_bytes),
        };
        let text_error = |_| "FSOm to OBJ conversion admission exceeds output budget".to_owned();
        let mut offset = 0usize;
        for geometry in geometries() {
            for vertex in &geometry.vertices {
                let p = vertex.position.map(F32Bits::get);
                let n = vertex.normal.map(F32Bits::get);
                let uv = [
                    vertex.texture_coordinate[0].get(),
                    1.0 - vertex.texture_coordinate[1].get(),
                ];
                if !uv[1].is_finite() {
                    return Err("OBJ UV conversion overflow".into());
                }
                // These are the actual eventual field lengths; order does not
                // affect counting. The write target owns no String allocation.
                writeln!(
                    text,
                    "v {} {} {}\nvt {} {}\nvn {} {} {}",
                    p[0], p[1], p[2], uv[0], uv[1], n[0], n[1], n[2]
                )
                .map_err(text_error)?;
            }
            if !geometry.indices.is_empty() {
                // Source group names contain bounded numeric fields; this
                // conservative fixed header covers both o/usemtl lines.
                text.write_str("................................................................................................................................").map_err(text_error)?;
                for triangle in geometry.indices.chunks_exact(3) {
                    text.write_char('f').map_err(text_error)?;
                    for &index in triangle {
                        let index = offset
                            .checked_add(index as usize)
                            .and_then(|n| n.checked_add(1))
                            .ok_or("OBJ index conversion overflow")?;
                        write!(text, " {index}/{index}/{index}").map_err(text_error)?;
                    }
                    text.write_char('\n').map_err(text_error)?;
                }
            }
            offset = offset
                .checked_add(geometry.vertices.len())
                .ok_or("OBJ vertex offset overflow")?;
        }
        let mut model = Self {
            positions: Vec::new(),
            textures: Vec::new(),
            normals: Vec::new(),
            groups: BTreeMap::new(),
            original: Vec::new(),
        };
        let mut add = |name: String, geom: &FsomGeometry| -> Result<(), String> {
            let offset = model.positions.len();
            for v in &geom.vertices {
                model.positions.push(v.position.map(F32Bits::get));
                model.textures.push([
                    v.texture_coordinate[0].get(),
                    1.0 - v.texture_coordinate[1].get(),
                ]);
                model.normals.push(v.normal.map(F32Bits::get));
            }
            let mut triangles = Vec::new();
            for tri in geom.indices.chunks_exact(3) {
                triangles.push(std::array::from_fn(|n| {
                    let i = offset + tri[n] as usize;
                    ObjCorner {
                        position: i,
                        texture: i,
                        normal: Some(i),
                    }
                }));
            }
            if model.groups.insert(name, triangles).is_some() {
                return Err(
                    "duplicate source material identity cannot be exported losslessly".into(),
                );
            }
            Ok(())
        };
        for (dynamic, group) in mesh.groups.iter().enumerate() {
            for geom in group {
                let name = if geom.pixel_direction == u16::MAX {
                    format!("{dynamic}_TEX_{}", geom.pixel_sprite)
                } else {
                    format!(
                        "{dynamic}_SPR_rot{}_{}",
                        geom.pixel_direction, geom.pixel_sprite
                    )
                };
                add(name, geom)?;
            }
        }
        if let Some(mask) = &mesh.depth_mask {
            add(
                if mesh.mask_type == 2 {
                    "DEPTH_MASK_PORTAL"
                } else {
                    "DEPTH_MASK"
                }
                .into(),
                mask,
            )?;
        }
        model.original = model.encode(limits)?;
        Ok(model)
    }
}

fn bounds(points: &[[f32; 3]]) -> Result<[[F32Bits; 3]; 2], String> {
    let mut min = points.first().copied().unwrap_or([0.0; 3]);
    let mut max = min;
    for p in points {
        for i in 0..3 {
            if !p[i].is_finite() {
                return Err("nonfinite OBJ position".into());
            }
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    Ok([bits(min), bits(max)])
}
fn generate_normals(vertices: &mut [FsomVertex], indices: &[i32]) -> Result<(), String> {
    let mut normals = vec![[0.0f32; 3]; vertices.len()];
    for face in indices.chunks_exact(3) {
        let a = vertices[face[0] as usize].position.map(F32Bits::get);
        let b = vertices[face[1] as usize].position.map(F32Bits::get);
        let c = vertices[face[2] as usize].position.map(F32Bits::get);
        let v1 = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
        let v2 = std::array::from_fn::<_, 3, _>(|i| c[i] - b[i]);
        let cross = [
            v1[1] * v2[2] - v1[2] * v2[1],
            v1[2] * v2[0] - v1[0] * v2[2],
            v1[0] * v2[1] - v1[1] * v2[0],
        ];
        for index in face {
            for (n, c) in normals[*index as usize].iter_mut().zip(cross) {
                *n += c;
            }
        }
    }
    for (vertex, normal) in vertices.iter_mut().zip(normals) {
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length == 0.0 || !length.is_finite() {
            return Err("degenerate OBJ normal requires an authored finite normal".into());
        }
        vertex.normal = bits(normal.map(|v| v / length));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MtlMaterial {
    pub name: String,
    pub diffuse: [f32; 3],
    pub diffuse_map: Option<String>,
    pub alpha_map: Option<String>,
}
pub fn decode_mtl(bytes: &[u8], limits: &Limits) -> Result<Vec<MtlMaterial>, String> {
    admit(bytes.len(), limits)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "MTL must be UTF-8")?;
    let mut result: Vec<MtlMaterial> = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        if line.len() > limits.max_string_bytes {
            return Err("MTL line limit exceeded".into());
        }
        let f: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if f.is_empty() {
            continue;
        }
        if f[0] == "newmtl" {
            if f.len() != 2 {
                return Err("MTL material requires one name".into());
            }
            name(f[1], limits)?;
            if !seen.insert(f[1]) {
                return Err("duplicate MTL material".into());
            }
            if result.len() >= limits.max_entries {
                return Err("MTL material limit exceeded".into());
            }
            result.push(MtlMaterial {
                name: f[1].into(),
                diffuse: [1.0; 3],
                diffuse_map: None,
                alpha_map: None,
            });
            continue;
        }
        let material = result.last_mut().ok_or("MTL property before material")?;
        match f[0] {
            "Kd" | "Ka" | "Ks" => {
                if f.len() != 4 {
                    return Err("MTL color needs RGB".into());
                }
                let color = [float(f[1])?, float(f[2])?, float(f[3])?];
                if f[0] == "Kd" {
                    material.diffuse = color;
                }
            }
            "Ns" | "illum" | "d" | "Tr" => {
                if f.len() != 2 {
                    return Err("MTL scalar property malformed".into());
                }
                float(f[1])?;
            }
            "map_Kd" | "map_d" => {
                if f.len() != 2 {
                    return Err("MTL maps require one local filename".into());
                }
                name(f[1], limits)?;
                if f[1] == "." || f[1] == ".." {
                    return Err("invalid texture filename".into());
                }
                if f[0] == "map_Kd" {
                    material.diffuse_map = Some(f[1].into());
                } else {
                    material.alpha_map = Some(f[1].into());
                }
            }
            _ => return Err(format!("unsupported MTL directive {}", f[0])),
        }
    }
    Ok(result)
}

pub(crate) fn bits(v: [f32; 3]) -> [F32Bits; 3] {
    v.map(F32Bits::from_f32)
}
