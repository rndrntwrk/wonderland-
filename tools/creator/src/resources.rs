// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
use crate::sha256;
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic, Limits,
};

/// Keeps the original envelope for exact no-op export, including unsupported resource-map metadata.
pub struct ResourceDocument {
    original: Vec<u8>,
    original_file: IffFile,
    file: IffFile,
}
#[derive(Clone, Debug)]
pub struct EditGuard {
    pub source_hash: String,
    pub resource_hash: String,
    pub format_version: Option<u32>,
}
#[derive(Clone, Debug)]
pub enum Edit {
    BhavBranch {
        instruction: usize,
        true_pointer: u8,
        false_pointer: u8,
    },
    BhavOperand {
        instruction: usize,
        operand: [u8; 8],
    },
    StringValue {
        set: usize,
        index: usize,
        value: String,
    },
    TuningConstant {
        index: usize,
        value: u16,
    },
    SlotOffset {
        index: usize,
        offset: [f32; 3],
    },
    UnknownBytes(Vec<u8>),
}
#[derive(Clone, Debug)]
pub struct CfgReport {
    pub unreachable: Vec<usize>,
    pub alternate_error_instructions: Vec<usize>,
}

/// Checks both branch destinations. Opcode operands are preserved, not executed or semantically invented.
pub fn validate_cfg(bhav: &semantic::Bhav) -> Result<CfgReport, String> {
    let n = bhav.instructions.len();
    for (i, inst) in bhav.instructions.iter().enumerate() {
        for pointer in [inst.true_pointer, inst.false_pointer] {
            if pointer < 253 && pointer as usize >= n {
                return Err(format!(
                    "BHAV instruction {i} has out-of-range branch {pointer} for {n} instructions"
                ));
            }
        }
    }
    let mut seen = vec![false; n];
    let mut todo = if n > 0 { vec![0usize] } else { vec![] };
    while let Some(i) = todo.pop() {
        if seen[i] {
            continue;
        }
        seen[i] = true;
        for p in [
            bhav.instructions[i].true_pointer,
            bhav.instructions[i].false_pointer,
        ] {
            if p < 253 {
                todo.push(p as usize);
            }
        }
    }
    Ok(CfgReport {
        unreachable: seen
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (!s).then_some(i))
            .collect(),
        alternate_error_instructions: bhav
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, v)| (v.true_pointer == 253 && v.false_pointer == 253).then_some(i))
            .collect(),
    })
}
impl ResourceDocument {
    pub fn import(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        limits.check_input(bytes).map_err(|e| e.to_string())?;
        limits
            .check_count(
                bytes
                    .len()
                    .checked_mul(5)
                    .ok_or("document allocation overflow")?,
                limits.max_total_decoded_bytes,
                0,
                "creator document copies",
            )
            .map_err(|e| e.to_string())?;
        let file = iff::decode(bytes, limits).map_err(|e| e.to_string())?;
        let footprint = bytes
            .len()
            .checked_add(
                file.chunks
                    .len()
                    .checked_mul(std::mem::size_of::<IffChunk>())
                    .ok_or("document allocation overflow")?,
            )
            .and_then(|n| n.checked_mul(5))
            .ok_or("document allocation overflow")?;
        limits
            .check_count(
                footprint,
                limits.max_total_decoded_bytes,
                0,
                "creator document copy and entry budget",
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            original: bytes.to_vec(),
            original_file: file.clone(),
            file,
        })
    }
    pub fn file(&self) -> &IffFile {
        &self.file
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        if self.file == self.original_file {
            limits
                .check_input(&self.original)
                .map_err(|e| e.to_string())?;
            return Ok(self.original.clone());
        }
        iff::encode(&self.file, limits).map_err(|e| e.to_string())
    }
    pub fn chunk(&self, key: ChunkKey) -> Result<&IffChunk, String> {
        self.file
            .chunks
            .iter()
            .find(|c| c.key == key)
            .ok_or_else(|| format!("resource {:?}/{} not found", key.kind, key.id))
    }
    pub fn guard(&self, key: ChunkKey, limits: &Limits) -> Result<EditGuard, String> {
        let c = self.chunk(key)?;
        Ok(EditGuard {
            source_hash: sha256(&self.export(limits)?),
            resource_hash: sha256(&c.data),
            format_version: resource_version(c, limits)?,
        })
    }
    pub fn validate(&self, limits: &Limits) -> Result<(), String> {
        for chunk in &self.file.chunks {
            validate_resource(chunk, limits)?;
        }
        Ok(())
    }
    /// All validation and serialization happen before publication; a rejected edit leaves this document unchanged.
    pub fn edit(
        &mut self,
        key: ChunkKey,
        guard: &EditGuard,
        edit: Edit,
        limits: &Limits,
    ) -> Result<(), String> {
        let actual = self.guard(key, limits)?;
        if guard.source_hash != actual.source_hash {
            return Err("source SHA-256 conflict".into());
        }
        if guard.resource_hash != actual.resource_hash {
            return Err("resource SHA-256 conflict".into());
        }
        if guard.format_version != actual.format_version {
            return Err("resource format-version conflict".into());
        }
        let chunk = self.chunk(key)?;
        let mut data = chunk.data.clone();
        match edit {
            Edit::BhavBranch {
                instruction,
                true_pointer,
                false_pointer,
            } => {
                require_kind(key, b"BHAV")?;
                let mut bhav = semantic::decode_bhav(&data, limits).map_err(|e| e.to_string())?;
                let inst = bhav
                    .instructions
                    .get_mut(instruction)
                    .ok_or("instruction index out of range")?;
                inst.true_pointer = true_pointer;
                inst.false_pointer = false_pointer;
                validate_cfg(&bhav)?;
                data = semantic::encode_bhav(&bhav, limits).map_err(|e| e.to_string())?;
            }
            Edit::BhavOperand {
                instruction,
                operand,
            } => {
                require_kind(key, b"BHAV")?;
                let mut bhav = semantic::decode_bhav(&data, limits).map_err(|e| e.to_string())?;
                bhav.instructions
                    .get_mut(instruction)
                    .ok_or("instruction index out of range")?
                    .operand = operand;
                validate_cfg(&bhav)?;
                data = semantic::encode_bhav(&bhav, limits).map_err(|e| e.to_string())?;
            }
            Edit::TuningConstant { index, value } => {
                require_kind(key, b"BCON")?;
                let mut tuning = semantic::decode_bcon(&data, limits).map_err(|e| e.to_string())?;
                *tuning
                    .constants
                    .get_mut(index)
                    .ok_or("tuning index out of range")? = value;
                data = semantic::encode_bcon(&tuning, limits).map_err(|e| e.to_string())?;
            }
            Edit::StringValue { set, index, value } => {
                data = edit_string(key, &data, set, index, &value, limits)?;
            }
            Edit::SlotOffset { index, offset } => {
                data = edit_slot(key, &data, index, offset, limits)?;
            }
            Edit::UnknownBytes(value) => {
                if supported_kind(key.kind) {
                    return Err(
                        "raw replacement is restricted to unknown resource kinds; use a typed edit"
                            .into(),
                    );
                }
                limits
                    .check_count(
                        value.len(),
                        limits.max_resource_bytes,
                        0,
                        "replacement resource bytes",
                    )
                    .map_err(|e| e.to_string())?;
                data = value;
            }
        }
        let mut candidate = self.file.clone();
        candidate
            .chunks
            .iter_mut()
            .find(|c| c.key == key)
            .unwrap()
            .data = data;
        validate_resource(
            candidate.chunks.iter().find(|c| c.key == key).unwrap(),
            limits,
        )?;
        let current_size = candidate.chunks.iter().try_fold(64usize, |n, c| {
            n.checked_add(76)
                .and_then(|n| n.checked_add(c.data.len()))
                .ok_or("edited document size overflow")
        })?;
        let budget = self
            .original
            .len()
            .max(current_size)
            .checked_add(
                candidate
                    .chunks
                    .len()
                    .checked_mul(std::mem::size_of::<IffChunk>())
                    .ok_or("edited entry allocation overflow")?,
            )
            .and_then(|n| n.checked_mul(5))
            .ok_or("edited document allocation overflow")?;
        limits
            .check_count(
                budget,
                limits.max_total_decoded_bytes,
                0,
                "creator editing copies",
            )
            .map_err(|e| e.to_string())?;
        // A no-op edit is valid even when resource-map metadata is opaque.
        if candidate != self.original_file {
            iff::encode(&candidate, limits).map_err(|e| e.to_string())?;
        }
        self.file = candidate;
        Ok(())
    }
}
fn require_kind(key: ChunkKey, kind: &[u8; 4]) -> Result<(), String> {
    if &key.kind != kind {
        Err("edit does not match resource kind".into())
    } else {
        Ok(())
    }
}
fn supported_kind(kind: [u8; 4]) -> bool {
    matches!(
        &kind,
        b"BHAV" | b"BCON" | b"STR#" | b"CTSS" | b"TTAs" | b"SLOT" | b"OBJD"
    )
}
pub(crate) fn resource_version(chunk: &IffChunk, limits: &Limits) -> Result<Option<u32>, String> {
    Ok(match &chunk.key.kind {
        b"BHAV" => Some(
            semantic::decode_bhav(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .format_version as u32,
        ),
        b"STR#" | b"CTSS" | b"TTAs" => Some(
            semantic::decode_strings(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .format as u16 as u32,
        ),
        b"SLOT" => Some(
            wonderland_legacy_formats::sprites::decode_slot(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .version,
        ),
        b"OBJD" => Some(
            semantic::decode_objd(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .version,
        ),
        _ => None,
    })
}
pub(crate) fn validate_resource(chunk: &IffChunk, limits: &Limits) -> Result<(), String> {
    match &chunk.key.kind {
        b"BHAV" => {
            validate_cfg(&semantic::decode_bhav(&chunk.data, limits).map_err(|e| e.to_string())?)?;
        }
        b"BCON" => {
            semantic::decode_bcon(&chunk.data, limits).map_err(|e| e.to_string())?;
        }
        b"STR#" | b"CTSS" | b"TTAs" => {
            semantic::decode_strings(&chunk.data, limits).map_err(|e| e.to_string())?;
        }
        b"SLOT" => {
            wonderland_legacy_formats::sprites::decode_slot(&chunk.data, limits)
                .map_err(|e| e.to_string())?;
        }
        b"OBJD" => {
            semantic::decode_objd(&chunk.data, limits).map_err(|e| e.to_string())?;
        }
        _ => {}
    }
    Ok(())
}
fn edit_string(
    key: ChunkKey,
    data: &[u8],
    set: usize,
    index: usize,
    value: &str,
    limits: &Limits,
) -> Result<Vec<u8>, String> {
    if !matches!(&key.kind, b"STR#" | b"CTSS" | b"TTAs") {
        return Err("string edit does not match resource kind".into());
    }
    if value.len() > limits.max_string_bytes {
        return Err("edited string byte limit exceeded".into());
    }
    let mut strings = semantic::decode_strings(data, limits).map_err(|e| e.to_string())?;
    if semantic::encode_strings(&strings, limits).map_err(|e| e.to_string())? != data {
        return Err(
            "noncanonical string encoding cannot be edited without unrelated byte changes".into(),
        );
    }
    let item = strings
        .sets
        .get_mut(set)
        .and_then(|s| s.get_mut(index))
        .ok_or("string set or index out of range")?;
    item.value =
        semantic::LegacyString::from_text(value, item.value.encoding).map_err(|e| e.to_string())?;
    semantic::encode_strings(&strings, limits).map_err(|e| e.to_string())
}
fn edit_slot(
    key: ChunkKey,
    data: &[u8],
    index: usize,
    offset: [f32; 3],
    limits: &Limits,
) -> Result<Vec<u8>, String> {
    require_kind(key, b"SLOT")?;
    let mut slots =
        wonderland_legacy_formats::sprites::decode_slot(data, limits).map_err(|e| e.to_string())?;
    let slot = slots
        .slots
        .get_mut(index)
        .ok_or("slot index out of range")?;
    if offset.iter().any(|v| !v.is_finite()) {
        return Err("slot offset must be finite".into());
    }
    slot.offset = offset.map(wonderland_legacy_formats::vitaboy::F32Bits::from_f32);
    wonderland_legacy_formats::sprites::encode_slot(&slots, limits).map_err(|e| e.to_string())
}
