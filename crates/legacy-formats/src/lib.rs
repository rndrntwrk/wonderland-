// SPDX-License-Identifier: MPL-2.0
//! Safe, bounded readers for FreeSO's source-derived legacy asset formats.
#![forbid(unsafe_code)]

pub mod compression;
pub mod dbpf;
mod error;
pub mod far;
pub mod iff;
mod limits;
pub mod reader;

pub use error::{Error, ErrorKind, Result};
pub use limits::Limits;

// Payload modules are implemented by their independent package owners.
pub mod audio_meta;
pub mod semantic;
pub mod sprites;
pub mod vitaboy;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ContainerFormat {
    Far1a,
    Far1b,
    Far3,
    Dbpf,
}

/// Canonical archive identities retain all source identity components. Filename
/// bytes are never normalized or used as filesystem paths.
#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ResourceKey {
    Far1 {
        name: Vec<u8>,
    },
    Far3 {
        type_id: u32,
        file_id: u32,
    },
    Dbpf {
        type_id: u32,
        group_id: u32,
        instance_id: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Compression {
    None,
    Far3PersistQfs,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContainerEntry {
    pub key: ResourceKey,
    /// Optional FAR3 name bytes; FAR1 name bytes are part of its canonical key.
    pub name: Option<Vec<u8>>,
    pub offset: usize,
    pub stored_len: usize,
    pub decoded_len: usize,
    pub compression: Compression,
}

/// An immutable archive index borrowing its source, with deferred extraction.
#[derive(Debug)]
pub struct ContainerIndex<'a> {
    bytes: &'a [u8],
    format: ContainerFormat,
    entries: Vec<ContainerEntry>,
}

impl<'a> ContainerIndex<'a> {
    pub fn entries(&self) -> &[ContainerEntry] {
        &self.entries
    }
    pub fn format(&self) -> ContainerFormat {
        self.format
    }
    pub fn find(&self, key: &ResourceKey) -> Option<usize> {
        self.entries.iter().position(|e| &e.key == key)
    }
    pub fn extract(&self, entry_index: usize, limits: &Limits) -> Result<Vec<u8>> {
        limits.check_input(self.bytes)?;
        let entry = self.entries.get(entry_index).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                0,
                "archive entry index out of range",
            )
        })?;
        limits.check_count(
            entry.stored_len,
            limits.max_resource_bytes,
            entry.offset,
            "stored entry bytes",
        )?;
        limits.check_count(
            entry.decoded_len,
            limits.max_resource_bytes,
            entry.offset,
            "decoded entry bytes",
        )?;
        limits.check_count(
            entry.decoded_len,
            limits.max_total_decoded_bytes,
            entry.offset,
            "decoded entry total",
        )?;
        let range = checked_range(
            entry.offset,
            entry.stored_len,
            self.bytes.len(),
            "archive entry range",
        )?;
        let stored = &self.bytes[range];
        match entry.compression {
            Compression::None => Ok(stored.to_vec()),
            Compression::Far3PersistQfs => {
                let qfs = stored.get(9..).ok_or_else(|| {
                    Error::new(ErrorKind::Truncated, entry.offset, "FAR3 Persist prefix")
                })?;
                let output = compression::decompress_qfs(qfs, limits).map_err(|mut e| {
                    e.offset += entry.offset + 9;
                    e
                })?;
                if output.len() != entry.decoded_len {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        entry.offset,
                        "FAR3 decoded size differs from manifest",
                    ));
                }
                Ok(output)
            }
        }
    }
}

/// Selects a layout explicitly; FAR1a and FAR1b share the on-disk version 1.
pub fn index<'a>(
    bytes: &'a [u8],
    format: ContainerFormat,
    limits: &Limits,
) -> Result<ContainerIndex<'a>> {
    match format {
        ContainerFormat::Far1a => far::index_v1(bytes, far::Far1Variant::A, limits),
        ContainerFormat::Far1b => far::index_v1(bytes, far::Far1Variant::B, limits),
        ContainerFormat::Far3 => far::index_v3(bytes, limits),
        ContainerFormat::Dbpf => dbpf::index(bytes, limits),
    }
}

pub(crate) fn checked_range(
    offset: usize,
    length: usize,
    input_len: usize,
    context: &str,
) -> Result<std::ops::Range<usize>> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, context))?;
    if end > input_len {
        return Err(Error::new(ErrorKind::Truncated, offset, context));
    }
    Ok(offset..end)
}

pub(crate) fn finish_index<'a>(
    bytes: &'a [u8],
    format: ContainerFormat,
    entries: Vec<ContainerEntry>,
    header_len: usize,
    index_range: std::ops::Range<usize>,
    limits: &Limits,
) -> Result<ContainerIndex<'a>> {
    use std::collections::BTreeSet;
    limits.check_count(entries.len(), limits.max_entries, 0, "archive entry count")?;
    let range_count = entries
        .len()
        .checked_add(2)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "archive range count"))?;
    let mut ranges = Vec::with_capacity(range_count);
    ranges.push(0..header_len);
    if !index_range.is_empty() {
        ranges.push(index_range);
    }
    let mut keys = BTreeSet::new();
    let mut total = 0usize;
    for entry in &entries {
        if !keys.insert(&entry.key) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                entry.offset,
                "duplicate archive identity",
            ));
        }
        limits.check_count(
            entry.stored_len,
            limits.max_resource_bytes,
            entry.offset,
            "stored entry bytes",
        )?;
        limits.check_count(
            entry.decoded_len,
            limits.max_resource_bytes,
            entry.offset,
            "decoded entry bytes",
        )?;
        total = total.checked_add(entry.decoded_len).ok_or_else(|| {
            Error::new(ErrorKind::Overflow, entry.offset, "archive decoded total")
        })?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            entry.offset,
            "archive decoded total",
        )?;
        let range = checked_range(
            entry.offset,
            entry.stored_len,
            bytes.len(),
            "archive payload range",
        )?;
        if entry.offset < header_len {
            return Err(Error::new(
                ErrorKind::Overlap,
                entry.offset,
                "archive entry overlaps header",
            ));
        }
        if !range.is_empty() {
            ranges.push(range);
        }
    }
    ranges.sort_unstable_by_key(|r| (r.start, r.end));
    for pair in ranges.windows(2) {
        if pair[1].start < pair[0].end {
            return Err(Error::new(
                ErrorKind::Overlap,
                pair[1].start,
                "archive ranges overlap",
            ));
        }
    }
    Ok(ContainerIndex {
        bytes,
        format,
        entries,
    })
}
