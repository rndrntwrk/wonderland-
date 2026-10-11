// SPDX-License-Identifier: MPL-2.0
//! Original and effective resource views from the actual ordered PIFF resolver.
use crate::{hex, sha256, ResourceDocument};
use serde::Serialize;
use std::collections::BTreeMap;
use wonderland_content_ir::patches::{apply_patches, PatchFile, PatchProvenance};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey},
    Limits,
};

pub struct PatchInput<'a> {
    pub name: &'a str,
    pub is_user: bool,
    pub bytes: &'a [u8],
}
#[derive(Debug, Serialize)]
pub struct ResourcePatchRow {
    pub kind_hex: String,
    pub id: u16,
    pub source_ordinal: Option<usize>,
    pub effective_ordinal: Option<usize>,
    pub original_sha256: Option<String>,
    pub effective_sha256: Option<String>,
    pub changed: bool,
}
#[derive(Debug, Serialize)]
pub struct PatchView {
    pub schema: &'static str,
    pub source_name: String,
    pub source_sha256: String,
    pub effective_sha256: String,
    pub applied: Vec<PatchProvenance>,
    pub suppressed: Vec<PatchProvenance>,
    pub resources: Vec<ResourcePatchRow>,
    #[serde(skip)]
    effective: Vec<u8>,
}
impl PatchView {
    pub fn resolve(
        source_name: &str,
        bytes: &[u8],
        patches: &[PatchInput<'_>],
        limits: &Limits,
    ) -> Result<Self, String> {
        if patches.len() > limits.max_entries || source_name.len() > limits.max_string_bytes {
            return Err("patch view entry limit exceeded".into());
        }
        let total = patches.iter().try_fold(bytes.len(), |n, p| {
            n.checked_add(p.bytes.len()).ok_or("patch input overflow")
        })?;
        if total
            .checked_mul(16)
            .and_then(|n| n.checked_add(65536))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
        {
            return Err("patch view working-copy limit exceeded".into());
        }
        let original = ResourceDocument::import(bytes, limits)?;
        let mut candidates = Vec::new();
        for patch in patches {
            if patch.name.len() > limits.max_string_bytes {
                return Err("patch name limit exceeded".into());
            }
            candidates.push(PatchFile {
                name: patch.name.into(),
                is_user: patch.is_user,
                file: iff::decode(patch.bytes, limits).map_err(|e| e.to_string())?,
            });
        }
        let result = apply_patches(source_name, original.file(), &candidates, limits)
            .map_err(|e| e.to_string())?;
        // The source-aware writer updates indexed maps instead of publishing stale offsets.
        let effective =
            iff::encode_rebuilding_index(bytes, &result.file, limits).map_err(|e| e.to_string())?;
        let reopened = ResourceDocument::import(&effective, limits)?;
        let mut rows: BTreeMap<ChunkKey, ResourcePatchRow> = BTreeMap::new();
        for (ordinal, c) in original.file().chunks.iter().enumerate() {
            if rows
                .insert(
                    c.key,
                    ResourcePatchRow {
                        kind_hex: hex(&c.key.kind),
                        id: c.key.id,
                        source_ordinal: Some(ordinal),
                        effective_ordinal: None,
                        original_sha256: Some(sha256(&c.data)),
                        effective_sha256: None,
                        changed: true,
                    },
                )
                .is_some()
            {
                return Err("ambiguous source resource key".into());
            }
        }
        for (ordinal, c) in reopened.file().chunks.iter().enumerate() {
            let row = rows.entry(c.key).or_insert_with(|| ResourcePatchRow {
                kind_hex: hex(&c.key.kind),
                id: c.key.id,
                source_ordinal: None,
                effective_ordinal: None,
                original_sha256: None,
                effective_sha256: None,
                changed: true,
            });
            if row.effective_ordinal.is_some() {
                return Err("ambiguous effective resource key".into());
            }
            row.effective_ordinal = Some(ordinal);
            row.effective_sha256 = Some(sha256(&c.data));
            row.changed = row
                .source_ordinal
                .and_then(|index| original.file().chunks.get(index))
                .is_none_or(|s| s != c);
        }
        Ok(Self {
            schema: "wonderland.creator.patch-view.v1",
            source_name: source_name.into(),
            source_sha256: sha256(bytes),
            effective_sha256: sha256(&effective),
            applied: result.applied,
            suppressed: result.suppressed,
            resources: rows.into_values().collect(),
            effective,
        })
    }
    pub fn effective_bytes(&self) -> &[u8] {
        &self.effective
    }
    pub fn metadata_json(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        crate::json_support::encode(self, limits)
    }
}
