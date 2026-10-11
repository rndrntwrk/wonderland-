// SPDX-License-Identifier: MPL-2.0
//! Source-bound FSOm OBJ edits preserve unchanged float bits and gzip bytes.
use crate::sha256;
use wonderland_asset_cooker::interchange::obj::ObjModel;
use wonderland_legacy_formats::{
    reconstruction::{self, FsomMesh},
    vitaboy::F32Bits,
    Limits,
};

pub struct MeshOverrideDocument {
    bytes: Vec<u8>,
    mesh: FsomMesh,
}
/// The gzip trailer's claimed raw size is used only for admission. The actual
/// codec subsequently verifies DEFLATE extent, CRC and exact decoded size.
pub(crate) fn admit_fsom(bytes: &[u8], copies: usize, limits: &Limits) -> Result<(), String> {
    if bytes.len() < 18 || bytes.len() > limits.max_input_bytes.min(limits.max_resource_bytes) {
        return Err("FSOm source byte limit or header".into());
    }
    let plain = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap()) as usize;
    if plain > limits.max_resource_bytes
        || plain
            .checked_mul(copies)
            .and_then(|n| bytes.len().checked_mul(2).and_then(|b| n.checked_add(b)))
            .and_then(|n| n.checked_add(256 * 1024))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        return Err("FSOm decoded authoring workspace limit exceeded".into());
    }
    Ok(())
}
impl MeshOverrideDocument {
    pub fn import(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        admit_fsom(bytes, 64, limits)?;
        let mesh = reconstruction::decode_fsom(bytes, limits).map_err(|e| e.to_string())?;
        let plain =
            reconstruction::encode_fsom_payload(&mesh, limits).map_err(|e| e.to_string())?;
        if plain
            .len()
            .checked_mul(64)
            .and_then(|n| n.checked_add(bytes.len()))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
        {
            return Err("mesh authoring workspace limit exceeded".into());
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            mesh,
        })
    }
    pub fn mesh(&self) -> &FsomMesh {
        &self.mesh
    }
    pub fn source_sha256(&self) -> String {
        sha256(&self.bytes)
    }
    pub fn export(&self) -> &[u8] {
        &self.bytes
    }
    pub fn export_obj(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        ObjModel::from_fsom(&self.mesh, limits)?.encode(limits)
    }
    pub fn export_mtl(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        ObjModel::from_fsom(&self.mesh, limits)?.source_materials(limits)
    }
    /// Edit attributes in a source-bound OBJ. Object/material identity and face
    /// topology stay fixed. New topology can be authored through ObjModel::to_fsom.
    pub fn import_obj(
        &mut self,
        expected: &str,
        bytes: &[u8],
        limits: &Limits,
    ) -> Result<(), String> {
        if expected.to_ascii_lowercase() != self.source_sha256() {
            return Err("FSOm source SHA-256 conflict".into());
        }
        let baseline = ObjModel::from_fsom(&self.mesh, limits)?;
        let baseline_text = baseline.encode(limits)?;
        if bytes == baseline_text {
            return Ok(());
        }
        let baseline = ObjModel::decode(&baseline_text, limits)?;
        let edited = ObjModel::decode(bytes, limits)?;
        if baseline.groups != edited.groups
            || baseline.positions.len() != edited.positions.len()
            || baseline.normals.len() != edited.normals.len()
            || baseline.textures.len() != edited.textures.len()
        {
            return Err("source-bound OBJ topology or material identity changed; use explicit new-mesh import".into());
        }
        let mut candidate = self.mesh.clone();
        let mut ordinal = 0usize;
        let mut position_changed = false;
        for geometry in candidate
            .groups
            .iter_mut()
            .flatten()
            .chain(candidate.depth_mask.iter_mut())
        {
            for vertex in &mut geometry.vertices {
                for axis in 0..3 {
                    if edited.positions[ordinal][axis].to_bits()
                        != baseline.positions[ordinal][axis].to_bits()
                    {
                        vertex.position[axis] = F32Bits::from_f32(edited.positions[ordinal][axis]);
                        position_changed = true;
                    }
                    if edited.normals[ordinal][axis].to_bits()
                        != baseline.normals[ordinal][axis].to_bits()
                    {
                        vertex.normal[axis] = F32Bits::from_f32(edited.normals[ordinal][axis]);
                    }
                }
                for axis in 0..2 {
                    if edited.textures[ordinal][axis].to_bits()
                        != baseline.textures[ordinal][axis].to_bits()
                    {
                        vertex.texture_coordinate[axis] = F32Bits::from_f32(if axis == 0 {
                            edited.textures[ordinal][axis]
                        } else {
                            1.0 - edited.textures[ordinal][axis]
                        });
                    }
                }
                ordinal += 1;
            }
        }
        if position_changed {
            let points = candidate
                .groups
                .iter()
                .flatten()
                .flat_map(|g| &g.vertices)
                .chain(
                    candidate
                        .depth_mask
                        .iter()
                        .filter(|_| candidate.mask_type != 2)
                        .flat_map(|g| &g.vertices),
                );
            let mut minimum = None::<[f32; 3]>;
            let mut maximum = [0.0f32; 3];
            for vertex in points {
                let p = vertex.position.map(F32Bits::get);
                if let Some(min) = minimum.as_mut() {
                    for axis in 0..3 {
                        min[axis] = min[axis].min(p[axis]);
                        maximum[axis] = maximum[axis].max(p[axis]);
                    }
                } else {
                    minimum = Some(p);
                    maximum = p;
                }
            }
            candidate.bounds = [
                minimum.unwrap_or([0.0; 3]).map(F32Bits::from_f32),
                maximum.map(F32Bits::from_f32),
            ];
        }
        if candidate == self.mesh {
            return Ok(());
        }
        let bytes = reconstruction::encode_fsom(&candidate, limits).map_err(|e| e.to_string())?;
        if reconstruction::decode_fsom(&bytes, limits).map_err(|e| e.to_string())? != candidate {
            return Err("FSOm candidate reopen mismatch".into());
        }
        self.bytes = bytes;
        self.mesh = candidate;
        Ok(())
    }
}
