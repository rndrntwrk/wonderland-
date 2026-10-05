// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
use crate::sha256;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic, Limits,
};

/// Keeps the last published envelope for exact no-op export, including opaque resource maps.
pub struct ResourceDocument {
    source: Vec<u8>,
    file: IffFile,
}
#[derive(Clone, Debug)]
pub struct EditGuard {
    pub source_hash: String,
    pub resource_hash: String,
    pub format_version: Option<u32>,
}
/// A resource expectation within a transaction's single source snapshot.
#[derive(Clone, Debug)]
pub struct ResourceGuard {
    pub resource_hash: String,
    pub format_version: Option<u32>,
}
impl From<&EditGuard> for ResourceGuard {
    fn from(guard: &EditGuard) -> Self {
        Self {
            resource_hash: guard.resource_hash.clone(),
            format_version: guard.format_version,
        }
    }
}
#[derive(Clone, Debug)]
pub enum ResourceOperation {
    Add {
        chunk: IffChunk,
    },
    Remove {
        key: ChunkKey,
        expected: ResourceGuard,
    },
    Edit {
        key: ChunkKey,
        expected: ResourceGuard,
        edit: Edit,
    },
    SetMetadata {
        key: ChunkKey,
        expected: ResourceGuard,
        new_key: ChunkKey,
        flags: u16,
        label: [u8; 64],
    },
}
/// Operations address the same pretransaction snapshot. Each existing key may be
/// targeted once; additions require a key absent from that snapshot. Final keys
/// must be unique, so a swap requires explicit, guarded metadata operations.
#[derive(Clone, Debug)]
pub struct ResourceTransaction {
    pub source_hash: String,
    pub operations: Vec<ResourceOperation>,
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
    PaletteColor {
        index: usize,
        rgb: [u8; 3],
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
            source: bytes.to_vec(),
            file,
        })
    }
    pub fn file(&self) -> &IffFile {
        &self.file
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        limits
            .check_input(&self.source)
            .map_err(|e| e.to_string())?;
        Ok(self.source.clone())
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
            source_hash: sha256(&self.source),
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
    /// Compatibility wrapper for a one-operation transaction.
    pub fn edit(
        &mut self,
        key: ChunkKey,
        guard: &EditGuard,
        edit: Edit,
        limits: &Limits,
    ) -> Result<(), String> {
        self.transact(
            &ResourceTransaction {
                source_hash: guard.source_hash.clone(),
                operations: vec![ResourceOperation::Edit {
                    key,
                    expected: guard.into(),
                    edit,
                }],
            },
            limits,
        )
    }

    /// Check all snapshot guards, conflicts, payloads and writer output before
    /// publishing either bytes or parsed state. A failure leaves both unchanged.
    pub fn transact(
        &mut self,
        transaction: &ResourceTransaction,
        limits: &Limits,
    ) -> Result<(), String> {
        check_digest(&transaction.source_hash, &sha256(&self.source), "source")?;
        limits
            .check_input(&self.source)
            .map_err(|e| e.to_string())?;
        limits
            .check_count(
                transaction.operations.len(),
                limits.max_entries,
                0,
                "transaction operations",
            )
            .map_err(|e| e.to_string())?;
        let mut upper_size = self.source.len();
        let mut operation_bytes = transaction
            .operations
            .len()
            .checked_mul(std::mem::size_of::<ResourceOperation>())
            .ok_or("transaction allocation overflow")?;
        for operation in &transaction.operations {
            let extra = match operation {
                ResourceOperation::Add { chunk } => chunk.data.len().checked_add(76),
                ResourceOperation::Edit {
                    edit: Edit::UnknownBytes(bytes),
                    ..
                } => Some(bytes.len()),
                ResourceOperation::Edit {
                    edit: Edit::StringValue { value, .. },
                    ..
                } => Some(value.len()),
                _ => Some(0),
            }
            .ok_or("transaction size overflow")?;
            upper_size = upper_size
                .checked_add(extra)
                .ok_or("transaction size overflow")?;
            operation_bytes = operation_bytes
                .checked_add(extra)
                .ok_or("transaction allocation overflow")?;
        }
        // Charge operations plus source/candidate/codec copies before constructing
        // any candidate or decoding a supplied payload. The final writer enforces
        // actual resource, entry and encoded-size limits as well.
        let candidate_entry_bound = self
            .file
            .chunks
            .len()
            .checked_add(transaction.operations.len())
            .ok_or("transaction entry count overflow")?;
        check_edit_budget(upper_size, candidate_entry_bound, operation_bytes, limits)?;

        let source_keys: BTreeSet<_> = self.file.chunks.iter().map(|c| c.key).collect();
        let mut by_key = BTreeMap::new();
        let mut additions = BTreeSet::new();
        for operation in &transaction.operations {
            let (key, expected) = match operation {
                ResourceOperation::Add { chunk } => {
                    require_user_resource(chunk.key)?;
                    if source_keys.contains(&chunk.key) || !additions.insert(chunk.key) {
                        return Err("added resource key already exists or is duplicated".into());
                    }
                    validate_resource(chunk, limits)?;
                    continue;
                }
                ResourceOperation::Remove { key, expected }
                | ResourceOperation::Edit { key, expected, .. }
                | ResourceOperation::SetMetadata { key, expected, .. } => (*key, expected),
            };
            require_user_resource(key)?;
            if by_key.insert(key, operation).is_some() {
                return Err("multiple operations target the same source resource".into());
            }
            let chunk = self.chunk(key)?;
            check_digest(&expected.resource_hash, &sha256(&chunk.data), "resource")?;
            if expected.format_version != resource_version(chunk, limits)? {
                return Err("resource format-version conflict".into());
            }
            if let ResourceOperation::SetMetadata { new_key, .. } = operation {
                require_user_resource(*new_key)?;
            }
        }
        let mut final_keys = additions;
        for chunk in &self.file.chunks {
            let key = match by_key.get(&chunk.key).copied() {
                Some(ResourceOperation::Remove { .. }) => continue,
                Some(ResourceOperation::SetMetadata { new_key, .. }) => *new_key,
                _ => chunk.key,
            };
            if !final_keys.insert(key) {
                return Err("transaction produces a resource key collision".into());
            }
        }
        limits
            .check_count(
                final_keys.len(),
                limits.max_entries,
                0,
                "transaction final resources",
            )
            .map_err(|e| e.to_string())?;
        let mut candidate = IffFile {
            header: self.file.header,
            chunks: Vec::with_capacity(final_keys.len()),
        };
        for chunk in &self.file.chunks {
            let operation = by_key.get(&chunk.key).copied();
            if matches!(operation, Some(ResourceOperation::Remove { .. })) {
                continue;
            }
            let updated = match operation {
                Some(ResourceOperation::Edit { edit, .. }) => {
                    let updated = IffChunk {
                        key: chunk.key,
                        flags: chunk.flags,
                        label: chunk.label,
                        data: edit_payload(chunk, edit, limits)?,
                    };
                    validate_resource(&updated, limits)?;
                    updated
                }
                Some(ResourceOperation::SetMetadata {
                    new_key,
                    flags,
                    label,
                    ..
                }) => {
                    let updated = IffChunk {
                        key: *new_key,
                        flags: *flags,
                        label: *label,
                        data: chunk.data.clone(),
                    };
                    validate_resource(&updated, limits)?;
                    updated
                }
                _ => chunk.clone(),
            };
            candidate.chunks.push(updated);
        }
        for operation in &transaction.operations {
            if let ResourceOperation::Add { chunk } = operation {
                candidate.chunks.push(chunk.clone());
            }
        }
        // Exact no-ops also work when the source's map is opaque/unsupported.
        if candidate == self.file {
            return Ok(());
        }
        let bytes = iff::encode_rebuilding_index(&self.source, &candidate, limits)
            .map_err(|e| e.to_string())?;
        check_edit_budget(bytes.len(), candidate.chunks.len(), operation_bytes, limits)?;
        // Reopen before publication so future guards use the regenerated map and
        // header, never stale map bytes retained from an earlier transaction.
        let file = iff::decode(&bytes, limits).map_err(|e| e.to_string())?;
        self.source = bytes;
        self.file = file;
        Ok(())
    }
}

fn check_edit_budget(
    size: usize,
    count: usize,
    operations: usize,
    limits: &Limits,
) -> Result<(), String> {
    let budget = count
        .checked_mul(std::mem::size_of::<IffChunk>())
        .and_then(|n| n.checked_add(size))
        .and_then(|n| n.checked_mul(5))
        .and_then(|n| n.checked_add(operations))
        .ok_or("transaction allocation overflow")?;
    limits
        .check_count(
            budget,
            limits.max_total_decoded_bytes,
            0,
            "creator transaction copies",
        )
        .map_err(|e| e.to_string())
}

fn require_user_resource(key: ChunkKey) -> Result<(), String> {
    if key.kind == *b"rsmp" {
        Err(
            "rsmp resource maps are managed by the IFF writer and cannot be authored directly"
                .into(),
        )
    } else {
        Ok(())
    }
}

pub(crate) fn check_digest(expected: &str, actual: &str, kind: &str) -> Result<(), String> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "expected {kind} SHA-256 must contain 64 hexadecimal characters"
        ));
    }
    if !expected.eq_ignore_ascii_case(actual) {
        return Err(format!("{kind} SHA-256 conflict"));
    }
    Ok(())
}

fn edit_payload(chunk: &IffChunk, edit: &Edit, limits: &Limits) -> Result<Vec<u8>, String> {
    let key = chunk.key;
    let data = chunk.data.as_slice();
    match edit {
        Edit::BhavBranch {
            instruction,
            true_pointer,
            false_pointer,
        } => {
            require_kind(key, b"BHAV")?;
            let mut bhav = semantic::decode_bhav(data, limits).map_err(|e| e.to_string())?;
            let inst = bhav
                .instructions
                .get_mut(*instruction)
                .ok_or("instruction index out of range")?;
            inst.true_pointer = *true_pointer;
            inst.false_pointer = *false_pointer;
            validate_cfg(&bhav)?;
            semantic::encode_bhav(&bhav, limits).map_err(|e| e.to_string())
        }
        Edit::BhavOperand {
            instruction,
            operand,
        } => {
            require_kind(key, b"BHAV")?;
            let mut bhav = semantic::decode_bhav(data, limits).map_err(|e| e.to_string())?;
            bhav.instructions
                .get_mut(*instruction)
                .ok_or("instruction index out of range")?
                .operand = *operand;
            validate_cfg(&bhav)?;
            semantic::encode_bhav(&bhav, limits).map_err(|e| e.to_string())
        }
        Edit::TuningConstant { index, value } => {
            require_kind(key, b"BCON")?;
            let mut tuning = semantic::decode_bcon(data, limits).map_err(|e| e.to_string())?;
            *tuning
                .constants
                .get_mut(*index)
                .ok_or("tuning index out of range")? = *value;
            semantic::encode_bcon(&tuning, limits).map_err(|e| e.to_string())
        }
        Edit::StringValue { set, index, value } => {
            edit_string(key, data, *set, *index, value, limits)
        }
        Edit::SlotOffset { index, offset } => edit_slot(key, data, *index, *offset, limits),
        Edit::PaletteColor { index, rgb } => {
            require_kind(key, b"PALT")?;
            let mut palette = wonderland_legacy_formats::sprites::decode_palt(data, limits)
                .map_err(|e| e.to_string())?;
            let color = palette
                .colors
                .get_mut(*index)
                .ok_or("palette index out of range")?;
            color[..3].copy_from_slice(rgb);
            wonderland_legacy_formats::sprites::encode_palt(&palette, limits)
                .map_err(|e| e.to_string())
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
            Ok(value.clone())
        }
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
        b"BHAV"
            | b"BCON"
            | b"STR#"
            | b"CTSS"
            | b"TTAs"
            | b"SLOT"
            | b"OBJD"
            | b"TTAB"
            | b"GLOB"
            | b"PIFF"
            | b"PALT"
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
        b"TTAB" => semantic::decode_ttab(&chunk.data, limits)
            .map_err(|e| e.to_string())?
            .version
            .map(u32::from),
        b"PIFF" => Some(u32::from(
            semantic::decode_piff(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .version,
        )),
        b"PALT" => Some(
            wonderland_legacy_formats::sprites::decode_palt(&chunk.data, limits)
                .map_err(|e| e.to_string())?
                .version,
        ),
        _ => None,
    })
}
pub(crate) fn validate_resource(chunk: &IffChunk, limits: &Limits) -> Result<(), String> {
    if chunk.key.kind == *b"PALT" {
        wonderland_legacy_formats::sprites::decode_palt(&chunk.data, limits)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let semantic::DecodedSemantic::Bhav(bhav) =
        semantic::decode_semantic(chunk, limits).map_err(|e| e.to_string())?
    {
        validate_cfg(&bhav)?;
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
