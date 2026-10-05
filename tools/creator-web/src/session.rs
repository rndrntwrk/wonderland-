// SPDX-License-Identifier: MPL-2.0
//! Browser-independent session rules. The Creator library is the binary authority.
use serde::Serialize;
use serde_json::{json, Value};
use wonderland_creator::{hex, sha256, Edit, EditGuard, ResourceDocument, ResourceTransaction};
use wonderland_legacy_formats::{iff::ChunkKey, semantic, sprites, Limits};

pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_HISTORY_ENTRIES: usize = 64;
pub const PAGE_SIZE: usize = 100;
pub const MAX_INSPECTION_JSON_BYTES: usize = 1024 * 1024;
pub const MAX_SPRITE_JSON_BYTES: usize = 1024 * 1024;
const INSPECTION_MEMORY_RESERVE: usize = 16 * 1024 * 1024;

pub fn browser_limits() -> Limits {
    Limits {
        max_input_bytes: MAX_FILE_BYTES,
        max_resource_bytes: 4 * 1024 * 1024,
        max_total_decoded_bytes: 128 * 1024 * 1024,
        max_entries: 4096,
        max_string_bytes: 64 * 1024,
        max_pixels: 1024 * 1024,
        ..wonderland_creator::default_limits()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResourceRow {
    pub key: ChunkKey,
    pub kind: String,
    pub label: String,
    pub bytes: usize,
    pub flags: u16,
    pub label_hex: String,
}

#[derive(Clone, Debug)]
pub struct Inspection {
    pub row: ResourceRow,
    pub guard: EditGuard,
    pub format_version: Option<u32>,
    pub details: Value,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChangeRecord {
    pub label: String,
    pub before_sha256: String,
    pub after_sha256: String,
}

struct Revision {
    bytes: Vec<u8>,
    selected: Option<ChunkKey>,
    change: ChangeRecord,
}

pub struct EditorSession {
    document: Option<ResourceDocument>,
    filename: String,
    imported_sha256: String,
    source_sha256: String,
    selected: Option<ChunkKey>,
    undo: Vec<Revision>,
    redo: Vec<Revision>,
    history_limit: usize,
    limits: Limits,
    read_generation: u64,
    pending_read: Option<(u64, usize)>,
}

impl Default for EditorSession {
    fn default() -> Self {
        Self::new(browser_limits(), MAX_HISTORY_BYTES).expect("fixed browser limits")
    }
}

impl EditorSession {
    pub fn new(limits: Limits, history_limit: usize) -> Result<Self, String> {
        let cap = browser_limits();
        if limits.max_input_bytes > cap.max_input_bytes
            || limits.max_total_decoded_bytes > cap.max_total_decoded_bytes
            || limits.max_entries > cap.max_entries
            || limits.max_resource_bytes > cap.max_resource_bytes
            || limits.max_string_bytes > cap.max_string_bytes
            || limits.max_pixels > cap.max_pixels
            || limits.max_vertices > cap.max_vertices
            || limits.max_frames > cap.max_frames
            || limits.max_depth > cap.max_depth
            || history_limit > MAX_HISTORY_BYTES
        {
            return Err("session limits exceed the browser budget".into());
        }
        Ok(Self {
            document: None,
            filename: String::new(),
            imported_sha256: String::new(),
            source_sha256: String::new(),
            selected: None,
            undo: vec![],
            redo: vec![],
            history_limit,
            limits,
            read_generation: 0,
            pending_read: None,
        })
    }
    pub fn begin_open(&mut self, size: usize) -> Result<u64, String> {
        if size > self.limits.max_input_bytes {
            return Err(format!(
                "file exceeds the {} byte import limit",
                self.limits.max_input_bytes
            ));
        }
        self.read_generation = self
            .read_generation
            .checked_add(1)
            .ok_or("file read generation exhausted")?;
        self.pending_read = Some((self.read_generation, size));
        Ok(self.read_generation)
    }
    pub fn is_current_read(&self, generation: u64) -> bool {
        self.pending_read
            .is_some_and(|(current, _)| current == generation)
    }
    pub fn complete_open(
        &mut self,
        generation: u64,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let (current, declared_size) = self.pending_read.ok_or("no pending file read")?;
        if current != generation {
            return Err("file read was superseded".into());
        }
        if declared_size != bytes.len() {
            return Err("file changed size while being read".into());
        }
        if name.len() > 256 || name.chars().any(char::is_control) {
            return Err("invalid or overlong filename".into());
        }
        let previous = self
            .document
            .as_ref()
            .map(|d| {
                d.file()
                    .chunks
                    .iter()
                    .map(|c| c.data.len() + 76)
                    .sum::<usize>()
                    + 64
            })
            .unwrap_or(0);
        let mut limits = self.limits;
        let retained = previous
            .checked_mul(4)
            .and_then(|n| n.checked_add(self.history_bytes()))
            .ok_or("session memory overflow")?;
        limits.max_total_decoded_bytes = limits
            .max_total_decoded_bytes
            .checked_sub(retained)
            .ok_or("session import memory budget exceeded")?;
        let candidate = ResourceDocument::import(bytes, &limits)?;
        let digest = sha256(bytes);
        let selected = candidate.file().chunks.first().map(|c| c.key);
        self.document = Some(candidate);
        self.filename = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("resource.iff")
            .to_owned();
        self.imported_sha256 = digest.clone();
        self.source_sha256 = digest;
        self.selected = selected;
        self.undo.clear();
        self.redo.clear();
        self.pending_read = None;
        Ok(())
    }
    pub fn loaded(&self) -> bool {
        self.document.is_some()
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn selected(&self) -> Option<ChunkKey> {
        self.selected
    }
    pub fn is_dirty(&self) -> bool {
        self.source_sha256 != self.imported_sha256
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    pub fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|r| r.bytes.len())
            .sum()
    }
    pub fn changes(&self) -> Vec<ChangeRecord> {
        self.undo.iter().rev().map(|r| r.change.clone()).collect()
    }
    pub fn resource_count(&self) -> usize {
        self.document
            .as_ref()
            .map(|d| d.file().chunks.len())
            .unwrap_or(0)
    }
    pub fn export(&self) -> Result<Vec<u8>, String> {
        self.document
            .as_ref()
            .ok_or("open an IFF first")?
            .export(&self.limits)
    }
    pub fn export_name(&self) -> String {
        let base = self
            .filename
            .strip_suffix(".iff")
            .or_else(|| self.filename.strip_suffix(".IFF"))
            .unwrap_or(&self.filename);
        let safe: String = base
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "-_. ".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{safe}.edited.iff")
    }
    pub fn select(&mut self, key: ChunkKey) -> Result<(), String> {
        self.document
            .as_ref()
            .ok_or("open an IFF first")?
            .chunk(key)?;
        self.selected = Some(key);
        Ok(())
    }
    pub fn rows(
        &self,
        query: &str,
        offset: usize,
        count: usize,
    ) -> Result<Vec<ResourceRow>, String> {
        if query.len() > 256 || count > 200 {
            return Err("resource filter/page limit exceeded".into());
        }
        let query = query.to_lowercase();
        let Some(document) = &self.document else {
            return Ok(vec![]);
        };
        Ok(document
            .file()
            .chunks
            .iter()
            .map(row)
            .filter(|r| {
                query.is_empty()
                    || r.kind.to_lowercase().contains(&query)
                    || r.label.to_lowercase().contains(&query)
                    || r.key.id.to_string().contains(&query)
            })
            .skip(offset)
            .take(count)
            .collect())
    }
    pub fn inspect(&self) -> Result<Inspection, String> {
        let document = self.document.as_ref().ok_or("open an IFF first")?;
        let key = self.selected.ok_or("select a resource")?;
        let chunk = document.chunk(key)?;
        // Invalid known payloads still have an honest raw inspection surface.
        let guard_result = document.guard(key, &self.limits);
        let guard = guard_result.clone().unwrap_or(EditGuard {
            source_hash: self.source_sha256.clone(),
            resource_hash: sha256(&chunk.data),
            format_version: None,
        });
        let decoded = self.operation_limits().and_then(|mut limits| {
            limits.max_total_decoded_bytes = limits.max_total_decoded_bytes
                .checked_sub(INSPECTION_MEMORY_RESERVE).ok_or("session inspection memory budget exceeded")?;
            if key.kind == *b"SPR2" {
                // A header alone does not make an SPR2 payload editable. This
                // validates its complete planes, exact encoding and real PALT
                // dependencies before exposing the source-bound authoring path.
                let _package = document.export_sprite(key.id, &limits)?;
                let version = u32::from_le_bytes(chunk.data[..4].try_into().unwrap());
                Ok(json!({"sprite_package":true,"format_version":version,"message":"Export the source-bound sprite package to edit exact indexed, alpha and depth planes."}))
            } else {
                inspection_details(chunk.key, &chunk.data, &limits)
            }
        });
        let error = guard_result
            .err()
            .or_else(|| decoded.as_ref().err().cloned());
        let format_version = guard.format_version.or_else(|| {
            decoded
                .as_ref()
                .ok()
                .and_then(|details| details["format_version"].as_u64())
                .and_then(|version| u32::try_from(version).ok())
        });
        Ok(Inspection {
            row: row(chunk),
            guard,
            format_version,
            details: decoded.unwrap_or_else(|_| raw_details(&chunk.data)),
            error,
        })
    }
    fn operation_limits(&self) -> Result<Limits, String> {
        let mut limits = self.limits;
        // The old document remains live while the Creator library edits a
        // detached candidate. Reserve its source, payload capacities and chunk
        // vector, as well as history and the two encoded snapshots. The library
        // separately charges all of the candidate's working copies.
        let mut reserved = self
            .history_bytes()
            .checked_add(
                self.limits
                    .max_input_bytes
                    .checked_mul(3)
                    .ok_or("session memory overflow")?,
            )
            .ok_or("session memory overflow")?;
        if let Some(document) = &self.document {
            reserved = document
                .file()
                .chunks
                .capacity()
                .checked_mul(std::mem::size_of::<wonderland_legacy_formats::iff::IffChunk>())
                .and_then(|n| n.checked_add(reserved))
                .ok_or("session memory overflow")?;
            for chunk in &document.file().chunks {
                reserved = reserved
                    .checked_add(chunk.data.capacity())
                    .ok_or("session memory overflow")?;
            }
        }
        limits.max_total_decoded_bytes = limits
            .max_total_decoded_bytes
            .checked_sub(reserved)
            .ok_or("session working memory budget exceeded")?;
        Ok(limits)
    }
    fn mutate<F>(&mut self, label: &str, edit: F) -> Result<bool, String>
    where
        F: FnOnce(&mut ResourceDocument, &Limits) -> Result<(), String>,
    {
        if label.len() > 256 {
            return Err("history label exceeds limit".into());
        }
        let before = self.export()?;
        if before.len() > self.history_limit {
            return Err("edit exceeds the retained undo budget".into());
        }
        let limits = self.operation_limits()?;
        let mut candidate = ResourceDocument::import(&before, &limits)?;
        edit(&mut candidate, &limits)?;
        let after = candidate.export(&limits)?;
        let digest = sha256(&after);
        if digest == self.source_sha256 {
            return Ok(false);
        }
        // Every accepted edit must be reversible. An expanded current document
        // is retained by undo, so admit its size before publishing the candidate.
        if after.len() > self.history_limit {
            return Err("edit exceeds the retained redo budget".into());
        }
        let change = ChangeRecord {
            label: label.to_owned(),
            before_sha256: self.source_sha256.clone(),
            after_sha256: digest.clone(),
        };
        self.redo.clear();
        let next_restore_size = before.len().max(after.len());
        while !self.undo.is_empty()
            && (self.history_bytes() + next_restore_size > self.history_limit
                || self.undo.len() >= MAX_HISTORY_ENTRIES)
        {
            self.undo.remove(0);
        }
        self.undo.push(Revision {
            bytes: before,
            selected: self.selected,
            change,
        });
        self.document = Some(candidate);
        self.pending_read = None;
        self.source_sha256 = digest;
        if self
            .selected
            .is_none_or(|k| self.document.as_ref().unwrap().chunk(k).is_err())
        {
            self.selected = self
                .document
                .as_ref()
                .unwrap()
                .file()
                .chunks
                .first()
                .map(|c| c.key);
        }
        Ok(true)
    }
    pub fn apply(
        &mut self,
        key: ChunkKey,
        guard: &EditGuard,
        edit: Edit,
        label: &str,
    ) -> Result<bool, String> {
        self.mutate(label, |document, limits| {
            document.edit(key, guard, edit, limits)
        })
    }
    pub fn transact(
        &mut self,
        transaction: &ResourceTransaction,
        label: &str,
    ) -> Result<bool, String> {
        self.mutate(label, |document, limits| {
            document.transact(transaction, limits)
        })
    }
    pub fn apply_sprite_json(&mut self, bytes: &[u8]) -> Result<bool, String> {
        if bytes.len() > MAX_SPRITE_JSON_BYTES {
            return Err("sprite package exceeds the 1 MiB browser editing limit".into());
        }
        let limits = self.operation_limits()?;
        let package =
            wonderland_creator::editors::sprites::SpritePackage::from_json(bytes, &limits)?;
        self.mutate("Import sprite edit", |document, limits| {
            document.import_sprite(&package, limits).map(|_| ())
        })
    }
    pub fn sprite_json(&self) -> Result<String, String> {
        let key = self.selected.ok_or("select a sprite")?;
        if key.kind != *b"SPR2" {
            return Err("sprite authoring requires an SPR2 resource".into());
        }
        let document = self.document.as_ref().ok_or("open an IFF first")?;
        let limits = self.operation_limits()?;
        let package = document.export_sprite(key.id, &limits)?;
        let output_limits = Limits {
            max_resource_bytes: limits.max_resource_bytes.min(MAX_SPRITE_JSON_BYTES),
            ..limits
        };
        package.to_json(&output_limits)
    }
    pub fn undo(&mut self) -> Result<bool, String> {
        self.restore(false)
    }
    pub fn redo(&mut self) -> Result<bool, String> {
        self.restore(true)
    }
    fn restore(&mut self, redo: bool) -> Result<bool, String> {
        let source = if redo { &self.redo } else { &self.undo };
        let Some(target) = source.last() else {
            return Ok(false);
        };
        let current = self.export()?;
        if current.len() > self.history_limit {
            return Err("restore exceeds the retained history budget".into());
        }
        let candidate = ResourceDocument::import(&target.bytes, &self.operation_limits()?)?;
        let digest = sha256(&target.bytes);
        // All fallible work is complete. Evict farthest retained revisions when
        // differently sized documents otherwise prevent a valid next restore.
        let target = if redo {
            self.redo.pop().unwrap()
        } else {
            self.undo.pop().unwrap()
        };
        while self.history_bytes() + current.len() > self.history_limit {
            let (source, destination) = if redo {
                (&mut self.redo, &mut self.undo)
            } else {
                (&mut self.undo, &mut self.redo)
            };
            if !source.is_empty() {
                source.remove(0);
            } else {
                destination.remove(0);
            }
        }
        let replacement = Revision {
            bytes: current,
            selected: self.selected,
            change: target.change,
        };
        if redo {
            self.undo.push(replacement);
        } else {
            self.redo.push(replacement);
        }
        self.document = Some(candidate);
        self.source_sha256 = digest;
        self.selected = target.selected;
        self.pending_read = None;
        Ok(true)
    }
}

fn row(chunk: &wonderland_legacy_formats::iff::IffChunk) -> ResourceRow {
    let end = chunk
        .label
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(chunk.label.len());
    ResourceRow {
        key: chunk.key,
        kind: String::from_utf8_lossy(&chunk.key.kind).into_owned(),
        label: String::from_utf8_lossy(&chunk.label[..end]).into_owned(),
        bytes: chunk.data.len(),
        flags: chunk.flags,
        label_hex: hex(&chunk.label),
    }
}
fn raw_details(bytes: &[u8]) -> Value {
    json!({"preview_hex":hex(&bytes[..bytes.len().min(2048)]),"preview_truncated":bytes.len()>2048})
}
fn admit_preview(
    output: usize,
    records: usize,
    text_bytes: usize,
    record_bytes: usize,
) -> Result<(), String> {
    let memory = records
        .checked_mul(record_bytes)
        .and_then(|n| n.checked_add(text_bytes.checked_mul(8)?))
        .and_then(|n| n.checked_add(output.checked_mul(2)?))
        .ok_or("resource preview allocation overflow")?;
    if output > MAX_INSPECTION_JSON_BYTES || memory > INSPECTION_MEMORY_RESERVE {
        return Err("Typed preview exceeds the browser inspection budget. The raw resource remains available for exact export and native Creator inspection.".into());
    }
    Ok(())
}
fn inspection_details(key: ChunkKey, bytes: &[u8], limits: &Limits) -> Result<Value, String> {
    let result = match &key.kind {
        b"BHAV" => {
            let value = semantic::decode_bhav(bytes, limits).map_err(|e| e.to_string())?;
            admit_preview(
                128 + value.instructions.len() * 192,
                value.instructions.len(),
                0,
                2048,
            )?;
            let cfg = wonderland_creator::validate_cfg(&value)?;
            json!({"instructions":value.instructions.iter().enumerate().map(|(i,v)| json!({"index":i,"opcode":v.opcode,"true":v.true_pointer,"false":v.false_pointer,"operand_hex":hex(&v.operand)})).collect::<Vec<_>>(),"args":value.args,"locals":value.locals,"unreachable":cfg.unreachable,"alternate_error_instructions":cfg.alternate_error_instructions})
        }
        b"STR#" | b"CTSS" | b"TTAs" => {
            let value = semantic::decode_strings(bytes, limits).map_err(|e| e.to_string())?;
            let records = value.sets.iter().map(Vec::len).sum::<usize>() + value.unassigned.len();
            let text_bytes = value
                .sets
                .iter()
                .flatten()
                .chain(&value.unassigned)
                .try_fold(0usize, |n, item| {
                    n.checked_add(item.value.bytes.len())
                        .and_then(|n| n.checked_add(item.comment.bytes.len()))
                })
                .ok_or("string preview size overflow")?;
            let output = text_bytes
                .checked_mul(6)
                .and_then(|n| n.checked_add(records.checked_mul(128)?))
                .and_then(|n| n.checked_add(value.sets.len().checked_mul(8)?))
                .and_then(|n| n.checked_add(64))
                .ok_or("string preview size overflow")?;
            admit_preview(output, records, text_bytes, 2048)?;
            json!({"sets":value.sets.iter().map(|s| s.iter().map(|v| json!({"language":v.language,"value":v.value.text(),"comment":v.comment.text()})).collect::<Vec<_>>()).collect::<Vec<_>>(),"unassigned":value.unassigned.iter().map(|v|json!({"language":v.language,"value":v.value.text(),"comment":v.comment.text()})).collect::<Vec<_>>(),"string_format":value.format})
        }
        b"BCON" => {
            let value = semantic::decode_bcon(bytes, limits).map_err(|e| e.to_string())?;
            admit_preview(32 + value.constants.len() * 8, value.constants.len(), 0, 64)?;
            json!({"constants":value.constants})
        }
        b"PALT" => {
            let value = sprites::decode_palt(bytes, limits).map_err(|e| e.to_string())?;
            admit_preview(32 + value.colors.len() * 24, value.colors.len(), 0, 512)?;
            json!({"colors":value.colors})
        }
        b"SLOT" => {
            let value = sprites::decode_slot(bytes, limits).map_err(|e| e.to_string())?;
            admit_preview(32 + value.slots.len() * 2048, value.slots.len(), 0, 4096)?;
            json!({"slots":value.slots})
        }
        _ => raw_details(bytes),
    };
    Ok(result)
}
