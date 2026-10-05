// Source-derived from FreeSO PIFFRegistry.cs and IffFile.cs.
// This Source Code Form is subject to the Mozilla Public License, v. 2.0.
// https://mozilla.org/MPL/2.0/
//! Ordered, transactional PIFF application with exact source-name matching.
use sha2::{Digest, Sha256};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic::{apply_piff_entry, decode_piff, Piff, PiffOperation},
    Error, ErrorKind, Limits, Result,
};

#[derive(Clone, Debug)]
pub struct PatchFile {
    pub name: String,
    pub is_user: bool,
    pub file: IffFile,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PatchProvenance {
    pub name: String,
    pub source_name: String,
    pub is_user: bool,
    /// Index in the explicitly supplied candidate sequence.
    pub order: usize,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct PatchApplication {
    pub file: IffFile,
    pub applied: Vec<PatchProvenance>,
    pub suppressed: Vec<PatchProvenance>,
}

pub fn iff_sha256(file: &IffFile, limits: &Limits) -> Result<[u8; 32]> {
    Ok(Sha256::digest(iff::encode(file, limits)?).into())
}

pub fn apply_patches(
    source_name: &str,
    source: &IffFile,
    candidates: &[PatchFile],
    limits: &Limits,
) -> Result<PatchApplication> {
    limits.check_count(
        source_name.len(),
        limits.max_string_bytes,
        0,
        "PIFF source name",
    )?;
    limits.check_count(
        candidates.len(),
        limits.max_entries,
        0,
        "PIFF candidate count",
    )?;
    // This also explicitly rejects opaque indexed envelopes before editing.
    iff::encode(source, limits)?;
    let mut decoded: Vec<(usize, Piff, PatchProvenance)> = Vec::new();
    let mut imported = source.chunks.iter().try_fold(0usize, |total, chunk| {
        total
            .checked_add(chunk.data.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "patch source size"))
    })?;
    for (order, patch) in candidates.iter().enumerate() {
        limits.check_count(patch.name.len(), limits.max_string_bytes, 0, "PIFF name")?;
        for chunk in &patch.file.chunks {
            imported = imported
                .checked_add(chunk.data.len())
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "PIFF import total"))?;
        }
        limits.check_count(
            imported,
            limits.max_total_decoded_bytes,
            0,
            "PIFF import total",
        )?;
        let sha256 = iff_sha256(&patch.file, limits)?;
        let descriptor = patch
            .file
            .chunks
            .iter()
            .find(|c| c.key.kind == *b"PIFF")
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    0,
                    "patch envelope has no PIFF descriptor",
                )
            })?;
        let piff = decode_piff(&descriptor.data, limits)?;
        let target = piff.source.text();
        if target == source_name {
            decoded.push((
                order,
                piff,
                PatchProvenance {
                    name: patch.name.clone(),
                    source_name: target,
                    is_user: patch.is_user,
                    order,
                    sha256,
                },
            ));
        }
    }
    let has_user = decoded.iter().any(|(_, _, p)| p.is_user);
    let mut result = PatchApplication {
        file: source.clone(),
        applied: Vec::new(),
        suppressed: Vec::new(),
    };
    for (order, descriptor, provenance) in decoded {
        if has_user && !provenance.is_user {
            result.suppressed.push(provenance);
            continue;
        }
        apply_one(
            &mut result.file,
            &descriptor,
            &candidates[order].file,
            limits,
        )?;
        // Validate the complete staged result before accepting this patch.
        iff::encode(&result.file, limits)?;
        result.applied.push(provenance);
    }
    Ok(result)
}

fn find_live(chunks: &[IffChunk], live: &[usize], key: ChunkKey) -> Option<usize> {
    live.iter().copied().find(|&index| chunks[index].key == key)
}

fn apply_one(file: &mut IffFile, patch: &Piff, envelope: &IffFile, limits: &Limits) -> Result<()> {
    let mut chunks = std::mem::take(&mut file.chunks);
    let mut live: Vec<usize> = (0..chunks.len()).collect();
    let mut moves = Vec::new();
    for entry in &patch.entries {
        let key = ChunkKey {
            kind: entry.kind,
            id: entry.id,
        };
        let Some(index) = find_live(&chunks, &live, key) else {
            continue;
        }; // source skips missing targets
        match &entry.operation {
            PiffOperation::Remove => live.retain(|&i| i != index),
            PiffOperation::Add => {} // descriptor only; payload chunks are added below
            PiffOperation::Patch { label, new_id, .. } => {
                chunks[index].data = apply_piff_entry(entry, &chunks[index].data, limits)?;
                if !label.bytes.is_empty() {
                    // IFF label output is ASCII in IoWriter.WriteCString.
                    let label_text = label.text();
                    let count = label_text.chars().count();
                    if count > 64 {
                        return Err(Error::new(
                            ErrorKind::LimitExceeded,
                            0,
                            "PIFF replacement label exceeds IFF label field",
                        ));
                    }
                    let mut encoded = [0; 64];
                    for (i, c) in label_text.chars().enumerate() {
                        encoded[i] = if c.is_ascii() { c as u8 } else { b'?' };
                    }
                    chunks[index].label = encoded;
                }
                // e.ChunkFlags is read by C# but not applied to existing chunks.
                if entry.id != *new_id {
                    moves.push((index, *new_id));
                }
            }
        }
    }
    for (index, new_id) in moves {
        let old_key = chunks[index].key;
        if old_key.id == new_id {
            continue;
        }
        let target = find_live(
            &chunks,
            &live,
            ChunkKey {
                kind: old_key.kind,
                id: new_id,
            },
        );
        live.retain(|&i| i != index && Some(i) != target);
        if find_live(&chunks, &live, old_key).is_some() {
            return Err(Error::new(
                ErrorKind::Duplicate,
                0,
                "PIFF deferred move would duplicate a chunk",
            ));
        }
        chunks[index].key.id = new_id;
        live.push(index);
        if let Some(target) = target {
            chunks[target].key.id = old_key.id;
            live.push(target);
        }
    }
    for chunk in &envelope.chunks {
        if chunk.key.kind == *b"PIFF" || find_live(&chunks, &live, chunk.key).is_some() {
            continue;
        }
        limits.check_count(
            live.len() + 1,
            limits.max_entries,
            0,
            "PIFF resulting chunks",
        )?;
        limits.check_count(
            chunk.data.len(),
            limits.max_resource_bytes,
            0,
            "PIFF added chunk bytes",
        )?;
        live.push(chunks.len());
        chunks.push(chunk.clone());
    }
    let mut slots: Vec<Option<IffChunk>> = chunks.into_iter().map(Some).collect();
    let mut output = Vec::with_capacity(live.len());
    let mut total = 0usize;
    for index in live {
        let chunk = slots[index]
            .take()
            .ok_or_else(|| Error::new(ErrorKind::Duplicate, 0, "PIFF internal live chunk"))?;
        total = total
            .checked_add(chunk.data.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "PIFF output total"))?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            0,
            "PIFF resulting data",
        )?;
        output.push(chunk);
    }
    file.chunks = output;
    Ok(())
}
