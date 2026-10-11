// SPDX-License-Identifier: MPL-2.0
//! Resource-map layouts are grounded in the bundled `srcs.zip` reader/writer:
//! `iff.cpp:98-202`, `mk_iff.cpp:374-406,410-467`, and the legacy C# reader
//! `Other/tools/SimsLib/SimsLib/IFF/Old/Iff.cs:220-308`.
use super::{map_offset, ChunkKey, IffChunk, IffFile};
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum SizeField {
    Zero,
    ChunkBytes,
    AfterSignatureBytes,
}

struct Entry {
    key: ChunkKey,
    padding: Option<u8>,
}

struct Group {
    kind: [u8; 4],
    entries: Vec<Entry>,
}

struct ResourceMap {
    version: u32,
    size_field: SizeField,
    groups: Vec<Group>,
}

struct OutputGroup {
    kind: [u8; 4],
    entries: Vec<(usize, Option<u8>)>,
}

pub(super) struct RebuiltIndex {
    pub chunk_index: usize,
    pub offset: u32,
    pub data: Vec<u8>,
    pub file_size: usize,
}

fn unsupported(context: &str) -> Error {
    Error::new(ErrorKind::UnsupportedVersion, 60, context)
}

fn only_map(file: &IffFile) -> Result<usize> {
    let mut maps = file
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, chunk)| chunk.key.kind == *b"rsmp");
    let index = maps
        .next()
        .map(|(index, _)| index)
        .ok_or_else(|| unsupported("indexed IFF must retain its source resource map"))?;
    if maps.next().is_some() {
        return Err(unsupported("multiple IFF resource maps are ambiguous"));
    }
    Ok(index)
}

fn label(chunk: &IffChunk) -> &[u8] {
    let length = chunk.label.iter().position(|&b| b == 0).unwrap_or(64);
    &chunk.label[..length]
}

fn checked_add(value: usize, amount: usize, at: usize, context: &str) -> Result<usize> {
    value
        .checked_add(amount)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, at, context))
}

/// Allocation-free first pass. Collection plans below use at most one slot
/// per source/output chunk: counts in the map are checked against this source
/// count before any map-derived reservation. The allowance covers overlapping
/// BTree nodes, Vec growth/minimum capacity, IffChunk copies, keys, type groups,
/// entries, inclusion markers and offsets on the supported 32/64-bit targets.
/// It is intentionally conservative, rather than an allocator-specific count.
pub(super) fn check_workspace(original: &[u8], edited: &IffFile, limits: &Limits) -> Result<()> {
    limits.check_input(original)?;
    limits.check_count(
        edited.chunks.len(),
        limits.max_entries,
        64,
        "IFF chunk count",
    )?;
    let mut reader = Reader::new(original);
    let header = reader.read_bytes(64)?;
    let mut indexed = header[60..64] != [0; 4];
    let mut source_count = 0usize;
    while reader.remaining() != 0 {
        let start = reader.position();
        source_count = checked_add(source_count, 1, start, "source IFF chunk count")?;
        limits.check_count(
            source_count,
            limits.max_entries,
            start,
            "source IFF chunk count",
        )?;
        indexed |= reader.read_bytes(4)? == b"rsmp";
        let length = reader.u32_be()? as usize;
        if length < 76 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                start + 4,
                "IFF chunk shorter than its header",
            ));
        }
        reader.skip(length - 8)?;
    }
    indexed |= edited.chunks.iter().any(|chunk| chunk.key.kind == *b"rsmp");
    let mut output_bytes = 64usize;
    let mut map_bytes = if indexed { 20usize } else { 0 };
    for chunk in &edited.chunks {
        output_bytes = checked_add(output_bytes, 76, 60, "IFF workspace output")?;
        if !indexed || chunk.key.kind != *b"rsmp" {
            output_bytes = checked_add(output_bytes, chunk.data.len(), 60, "IFF workspace output")?;
            if indexed {
                // Upper bound: each resource may create a new type group;
                // v1 is at least as large as v0 for every 0..=64-byte name.
                map_bytes =
                    checked_add(map_bytes, 19 + label(chunk).len(), 60, "IFF workspace map")?;
            }
        }
    }
    output_bytes = checked_add(output_bytes, map_bytes, 60, "IFF workspace output")?;
    let slots = checked_add(
        source_count,
        edited.chunks.len(),
        60,
        "IFF workspace collections",
    )?;
    let collections = slots
        .checked_mul(1024)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 60, "IFF workspace collection bytes"))?;
    let mut workspace = checked_add(original.len(), collections, 60, "IFF workspace bytes")?;
    workspace = checked_add(workspace, output_bytes, 60, "IFF workspace bytes")?;
    workspace = checked_add(workspace, map_bytes, 60, "IFF workspace bytes")?;
    limits.check_count(
        workspace,
        limits.max_total_decoded_bytes,
        60,
        "IFF rebuild workspace bytes",
    )
}

/// This validation is against the ORIGINAL layout, never the edited offsets.
fn read_original(file: &IffFile, map_index: usize, limits: &Limits) -> Result<ResourceMap> {
    let mut chunks_at = BTreeMap::new();
    let mut at = 64usize;
    let mut map_start = 0;
    for (index, chunk) in file.chunks.iter().enumerate() {
        if index == map_index {
            map_start = at;
        }
        chunks_at.insert(at, chunk);
        at = checked_add(at, 76, at, "IFF chunk offset")?;
        at = checked_add(at, chunk.data.len(), at, "IFF chunk offset")?;
    }
    if map_offset(&file.header) != map_start {
        return Err(unsupported(
            "source IFF header must point to its unique resource map",
        ));
    }
    let base = map_start + 76;
    let data = &file.chunks[map_index].data;
    read_payload(data, &chunks_at, file.chunks.len() - 1, limits).map_err(|mut error| {
        error.offset += base;
        error
    })
}

fn read_payload(
    data: &[u8],
    chunks_at: &BTreeMap<usize, &IffChunk>,
    resource_count: usize,
    limits: &Limits,
) -> Result<ResourceMap> {
    let mut reader = Reader::new(data);
    if reader.u32_le()? != 0 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            "nonzero resource-map reserved metadata",
        ));
    }
    let version = reader.u32_le()?;
    if version > 1 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            4,
            "resource-map version must be 0 or 1",
        ));
    }
    if reader.read_bytes(4)? != b"pmsr" {
        return Err(Error::new(
            ErrorKind::InvalidMagic,
            8,
            "resource-map validator is not pmsr",
        ));
    }
    let size = reader.u32_le()? as usize;
    // The original writer emits zero. The source corpus also has these two
    // exact length conventions; other nonzero metadata is not guessed away.
    let size_field = if size == 0 {
        SizeField::Zero
    } else if Some(size) == data.len().checked_add(76) {
        SizeField::ChunkBytes
    } else if Some(size) == data.len().checked_sub(12) {
        SizeField::AfterSignatureBytes
    } else {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            12,
            "unrecognized resource-map size metadata",
        ));
    };
    let type_count = reader.u32_le()? as usize;
    limits.check_count(
        type_count,
        limits.max_entries,
        16,
        "resource-map type count",
    )?;
    if type_count > resource_count {
        return Err(Error::new(
            ErrorKind::InvalidData,
            16,
            "resource-map type count exceeds original resource count",
        ));
    }
    if type_count > reader.remaining() / 8 {
        return Err(Error::new(
            ErrorKind::Truncated,
            16,
            "resource-map type headers exceed payload",
        ));
    }
    let mut groups = Vec::with_capacity(type_count);
    let mut kinds = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut entry_total = 0usize;
    for _ in 0..type_count {
        let group_start = reader.position();
        let mut kind = [0; 4];
        kind.copy_from_slice(reader.read_bytes(4)?);
        kind.reverse();
        if kind == *b"rsmp" {
            return Err(Error::new(
                ErrorKind::InvalidData,
                group_start,
                "resource maps must not index themselves",
            ));
        }
        if !kinds.insert(kind) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                group_start,
                "duplicate resource-map type group",
            ));
        }
        let count = reader.u32_le()? as usize;
        entry_total = checked_add(entry_total, count, group_start + 4, "resource-map entries")?;
        limits.check_count(
            entry_total,
            limits.max_entries,
            group_start + 4,
            "resource-map entry count",
        )?;
        if entry_total > resource_count {
            return Err(Error::new(
                ErrorKind::InvalidData,
                group_start + 4,
                "resource-map entry count exceeds original resource count",
            ));
        }
        if count == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                group_start + 4,
                "empty resource-map type group",
            ));
        }
        let minimum_entry = if version == 0 { 10 } else { 11 };
        if count > reader.remaining() / minimum_entry {
            return Err(Error::new(
                ErrorKind::Truncated,
                group_start + 4,
                "resource-map entries exceed payload",
            ));
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let entry_start = reader.position();
            let offset = reader.u32_le()? as usize;
            let id = reader.u16_le()?;
            if version == 1 && reader.u16_le()? != 0 {
                return Err(Error::new(
                    ErrorKind::UnsupportedVersion,
                    entry_start + 6,
                    "resource-map extended ID exceeds chunk ID width",
                ));
            }
            let flags = reader.u16_le()?;
            let (name, padding) = read_label(&mut reader, data, version, limits)?;
            let key = ChunkKey { kind, id };
            let chunk = chunks_at.get(&offset).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    entry_start,
                    "resource-map offset is not an original chunk boundary",
                )
            })?;
            if chunk.key != key || chunk.flags != flags || label(chunk) != name {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    entry_start,
                    "resource-map entry differs from its original chunk",
                ));
            }
            if !keys.insert(key) {
                return Err(Error::new(
                    ErrorKind::Duplicate,
                    entry_start,
                    "duplicate resource-map entry",
                ));
            }
            entries.push(Entry { key, padding });
        }
        groups.push(Group { kind, entries });
    }
    if reader.remaining() != 0 || keys.len() != resource_count {
        return Err(Error::new(
            ErrorKind::InvalidData,
            reader.position(),
            "resource map must cover every non-map chunk once without trailing data",
        ));
    }
    Ok(ResourceMap {
        version,
        size_field,
        groups,
    })
}

fn read_label<'a>(
    reader: &mut Reader<'a>,
    data: &'a [u8],
    version: u32,
    limits: &Limits,
) -> Result<(&'a [u8], Option<u8>)> {
    if version == 1 {
        let at = reader.position();
        let length = reader.u8()? as usize;
        check_label_length(length, at, limits)?;
        return Ok((reader.read_bytes(length)?, None));
    }
    let start = reader.position();
    loop {
        let at = reader.position();
        let pair = reader.read_bytes(2)?;
        if let Some(end) = pair.iter().position(|&b| b == 0) {
            let length = at + end - start;
            check_label_length(length, start, limits)?;
            return Ok((&data[start..start + length], (end == 0).then_some(pair[1])));
        }
        check_label_length(reader.position() - start, start, limits)?;
    }
}

fn check_label_length(length: usize, at: usize, limits: &Limits) -> Result<()> {
    limits.check_count(
        length,
        limits.max_string_bytes,
        at,
        "resource-map label bytes",
    )?;
    if length > 64 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            at,
            "resource-map label exceeds its chunk label width",
        ));
    }
    Ok(())
}

fn output_groups(map: &ResourceMap, edited: &IffFile, map_index: usize) -> Vec<OutputGroup> {
    let mut by_key = BTreeMap::new();
    let mut by_kind: BTreeMap<[u8; 4], Vec<usize>> = BTreeMap::new();
    let mut kind_order = Vec::new();
    for (index, chunk) in edited.chunks.iter().enumerate() {
        if index == map_index {
            continue;
        }
        by_key.insert(chunk.key, index);
        let entries = by_kind.entry(chunk.key.kind).or_insert_with(|| {
            kind_order.push(chunk.key.kind);
            Vec::new()
        });
        entries.push(index);
    }
    let mut included = vec![false; edited.chunks.len()];
    let mut groups = Vec::new();
    // Preserve source type/entry ordering and padding. New entries follow
    // surviving entries of their type; new types follow source types.
    for group in &map.groups {
        let mut entries = Vec::new();
        for entry in &group.entries {
            if let Some(&index) = by_key.get(&entry.key) {
                entries.push((index, entry.padding));
                included[index] = true;
            }
        }
        if let Some(indices) = by_kind.remove(&group.kind) {
            for index in indices {
                if !included[index] {
                    entries.push((index, None));
                }
            }
        }
        if !entries.is_empty() {
            groups.push(OutputGroup {
                kind: group.kind,
                entries,
            });
        }
    }
    for kind in kind_order {
        if let Some(indices) = by_kind.remove(&kind) {
            groups.push(OutputGroup {
                kind,
                entries: indices.into_iter().map(|index| (index, None)).collect(),
            });
        }
    }
    groups
}

pub(super) fn rebuild(
    original: &IffFile,
    edited: &IffFile,
    limits: &Limits,
) -> Result<RebuiltIndex> {
    let original_index = only_map(original)?;
    let chunk_index = only_map(edited)?;
    if original.header != edited.header
        || original.chunks[original_index] != edited.chunks[chunk_index]
    {
        return Err(unsupported(
            "indexed IFF header and resource-map bytes are writer-managed",
        ));
    }
    let map = read_original(original, original_index, limits)?;
    let groups = output_groups(&map, edited, chunk_index);
    limits.check_count(
        groups.len(),
        limits.max_entries,
        60,
        "resource-map type count",
    )?;
    let mut map_bytes = 20usize;
    for group in &groups {
        map_bytes = checked_add(map_bytes, 8, 60, "rebuilt resource-map bytes")?;
        for &(index, _) in &group.entries {
            let name = label(&edited.chunks[index]);
            check_label_length(name.len(), 60, limits)?;
            let entry_bytes = if map.version == 0 {
                8 + (name.len() + 2) / 2 * 2
            } else {
                11 + name.len()
            };
            map_bytes = checked_add(map_bytes, entry_bytes, 60, "rebuilt resource-map bytes")?;
        }
    }
    limits.check_count(
        map_bytes,
        limits.max_resource_bytes,
        60,
        "rebuilt resource-map bytes",
    )?;
    let map_chunk_bytes =
        u32::try_from(checked_add(map_bytes, 76, 60, "resource-map chunk bytes")?)
            .map_err(|_| Error::new(ErrorKind::Overflow, 60, "resource-map size exceeds u32"))?;
    let mut offsets = Vec::with_capacity(edited.chunks.len());
    let mut file_size = 64usize;
    let mut decoded_bytes = 0usize;
    for (index, chunk) in edited.chunks.iter().enumerate() {
        offsets.push(u32::try_from(file_size).map_err(|_| {
            Error::new(
                ErrorKind::Overflow,
                file_size,
                "IFF resource offset exceeds u32",
            )
        })?);
        let length = if index == chunk_index {
            map_bytes
        } else {
            chunk.data.len()
        };
        decoded_bytes = checked_add(
            decoded_bytes,
            length,
            file_size,
            "rebuilt IFF decoded bytes",
        )?;
        file_size = checked_add(file_size, 76, file_size, "rebuilt IFF bytes")?;
        file_size = checked_add(file_size, length, file_size, "rebuilt IFF bytes")?;
    }
    limits.check_count(
        decoded_bytes,
        limits.max_total_decoded_bytes,
        0,
        "rebuilt IFF decoded bytes",
    )?;
    limits.check_count(file_size, limits.max_input_bytes, 0, "rebuilt IFF bytes")?;
    let size = match map.size_field {
        SizeField::Zero => 0,
        SizeField::ChunkBytes => map_chunk_bytes,
        SizeField::AfterSignatureBytes => map_chunk_bytes - 88,
    };
    let mut data = Vec::with_capacity(map_bytes);
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&map.version.to_le_bytes());
    data.extend_from_slice(b"pmsr");
    data.extend_from_slice(&size.to_le_bytes());
    data.extend_from_slice(&(groups.len() as u32).to_le_bytes());
    for group in groups {
        let mut kind = group.kind;
        kind.reverse();
        data.extend_from_slice(&kind);
        data.extend_from_slice(&(group.entries.len() as u32).to_le_bytes());
        for (index, padding) in group.entries {
            let chunk = &edited.chunks[index];
            data.extend_from_slice(&offsets[index].to_le_bytes());
            data.extend_from_slice(&chunk.key.id.to_le_bytes());
            if map.version == 1 {
                data.extend_from_slice(&0u16.to_le_bytes());
            }
            data.extend_from_slice(&chunk.flags.to_le_bytes());
            let name = label(chunk);
            if map.version == 0 {
                data.extend_from_slice(name);
                data.push(0);
                if !data.len().is_multiple_of(2) {
                    data.push(padding.unwrap_or(0));
                }
            } else {
                data.push(name.len() as u8);
                data.extend_from_slice(name);
            }
        }
    }
    Ok(RebuiltIndex {
        chunk_index,
        offset: offsets[chunk_index],
        data,
        file_size,
    })
}
