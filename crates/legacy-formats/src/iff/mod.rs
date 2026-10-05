// SPDX-License-Identifier: MPL-2.0
//! Raw IFF 2.0/2.5 and PIFF envelopes; payload interpretation is separate.
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use std::collections::BTreeSet;

mod index;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ChunkKey {
    pub kind: [u8; 4],
    pub id: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IffChunk {
    pub key: ChunkKey,
    pub flags: u16,
    pub label: [u8; 64],
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IffFile {
    pub header: [u8; 64],
    pub chunks: Vec<IffChunk>,
}

fn validate_header(header: &[u8; 64]) -> Result<()> {
    let clean = || header[..60].iter().copied().filter(|&b| b != 0);
    if !clean().eq(
        b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1"
            .iter()
            .copied(),
    ) && !clean().eq(
        b"IFF FILE 2.0:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1"
            .iter()
            .copied(),
    ) {
        return Err(Error::new(
            if header.starts_with(b"IFF FILE ") {
                ErrorKind::UnsupportedVersion
            } else {
                ErrorKind::InvalidMagic
            },
            0,
            "expected IFF 2.0 or 2.5 identifier",
        ));
    }
    Ok(())
}

fn map_offset(header: &[u8; 64]) -> usize {
    u32::from_be_bytes([header[60], header[61], header[62], header[63]]) as usize
}

fn has_map(file: &IffFile) -> bool {
    map_offset(&file.header) != 0 || file.chunks.iter().any(|c| c.key.kind == *b"rsmp")
}

/// Reads every chunk, including unknown resources and PIFF payloads, without
/// executing patches or interpreting opaque resource maps.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<IffFile> {
    limits.check_input(bytes)?;
    let mut reader = Reader::new(bytes);
    let mut header = [0; 64];
    header.copy_from_slice(reader.read_bytes(64)?);
    validate_header(&header)?;
    let resource_map = map_offset(&header);
    if resource_map != 0 && resource_map < 64 {
        return Err(Error::new(
            ErrorKind::Overlap,
            60,
            "resource map overlaps IFF header",
        ));
    }
    if resource_map >= bytes.len() && resource_map != 0 {
        return Err(Error::new(
            ErrorKind::Truncated,
            60,
            "resource map outside input",
        ));
    }
    let mut chunks = Vec::new();
    let mut keys = BTreeSet::new();
    let mut total = 0usize;
    let mut found_map = resource_map == 0;
    while reader.remaining() != 0 {
        let start = reader.position();
        limits.check_count(
            chunks.len() + 1,
            limits.max_entries,
            start,
            "IFF chunk count",
        )?;
        let mut kind = [0; 4];
        kind.copy_from_slice(reader.read_bytes(4)?);
        let size = reader.u32_be()? as usize;
        let data_size = size.checked_sub(76).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                start + 4,
                "IFF chunk shorter than its header",
            )
        })?;
        limits.check_count(
            data_size,
            limits.max_resource_bytes,
            start + 4,
            "IFF chunk bytes",
        )?;
        total = total
            .checked_add(data_size)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, start, "IFF decoded total"))?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            start,
            "IFF decoded total",
        )?;
        let key = ChunkKey {
            kind,
            id: reader.u16_be()?,
        };
        let flags = reader.u16_be()?;
        let mut label = [0; 64];
        label.copy_from_slice(reader.read_bytes(64)?);
        let data = reader.read_bytes(data_size)?;
        if !keys.insert(key) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                start,
                "duplicate IFF type/id",
            ));
        }
        if start == resource_map {
            if kind != *b"rsmp" {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    60,
                    "resource map does not point to an rsmp chunk",
                ));
            }
            found_map = true;
        }
        chunks.push(IffChunk {
            key,
            flags,
            label,
            data: data.to_vec(),
        });
    }
    if !found_map {
        return Err(Error::new(
            ErrorKind::InvalidData,
            60,
            "resource map is not at a chunk boundary",
        ));
    }
    Ok(IffFile { header, chunks })
}

fn encoded_size(file: &IffFile, limits: &Limits) -> Result<usize> {
    validate_header(&file.header)?;
    limits.check_count(file.chunks.len(), limits.max_entries, 64, "IFF chunk count")?;
    let mut keys = BTreeSet::new();
    let mut size = 64usize;
    let mut total = 0usize;
    for chunk in &file.chunks {
        limits.check_count(
            chunk.data.len(),
            limits.max_resource_bytes,
            size,
            "IFF chunk bytes",
        )?;
        let chunk_size = chunk
            .data
            .len()
            .checked_add(76)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, size, "IFF chunk size"))?;
        if u32::try_from(chunk_size).is_err() {
            return Err(Error::new(
                ErrorKind::Overflow,
                size,
                "IFF chunk size exceeds u32",
            ));
        }
        total = total
            .checked_add(chunk.data.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, size, "IFF decoded total"))?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            size,
            "IFF decoded total",
        )?;
        if !keys.insert(chunk.key) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                size,
                "duplicate IFF type/id",
            ));
        }
        size = size
            .checked_add(chunk_size)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, size, "IFF encoded size"))?;
    }
    limits.check_count(size, limits.max_input_bytes, 0, "IFF encoded bytes")?;
    Ok(size)
}

/// Encodes envelopes without resource maps. Indexed edits require original
/// offset evidence: use [`IffDocument`] or [`encode_rebuilding_index`].
pub fn encode(file: &IffFile, limits: &Limits) -> Result<Vec<u8>> {
    let size = encoded_size(file, limits)?;
    if has_map(file) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            60,
            "indexed IFF encoding requires the original source document",
        ));
    }
    Ok(write_envelope(file, size, None))
}

/// Encodes an edited file against its exact source envelope. Untouched files,
/// including unsupported maps, retain their original bytes. Changed indexed
/// files require a complete, consistent version 0 or 1 resource map; its
/// offsets, counts, names and sizes are rebuilt from the edited chunk list.
///
/// The source map chunk and header are writer-managed: callers may relocate
/// the map but must retain their bytes until this call succeeds. Decode the
/// returned bytes to establish the baseline for a subsequent edit. This API
/// never repairs an inconsistent source map or silently drops indexing data.
///
/// `max_total_decoded_bytes` also bounds a conservative per-call workspace
/// plan: source clone, output, rebuilt map, and collection storage. Both the
/// supplied map payload and its replacement must satisfy resource limits, so
/// a tight limit may reject a shrinking edit whose old map exceeds that limit.
pub fn encode_rebuilding_index(
    original_bytes: &[u8],
    edited: &IffFile,
    limits: &Limits,
) -> Result<Vec<u8>> {
    index::check_workspace(original_bytes, edited, limits)?;
    encoded_size(edited, limits)?;
    let original = decode(original_bytes, limits)?;
    if original == *edited {
        return Ok(original_bytes.to_vec());
    }
    if !has_map(&original) {
        return encode(edited, limits);
    }
    let rebuilt = index::rebuild(&original, edited, limits)?;
    Ok(write_envelope(
        edited,
        rebuilt.file_size,
        Some((rebuilt.chunk_index, rebuilt.offset, &rebuilt.data)),
    ))
}

fn write_envelope(
    file: &IffFile,
    size: usize,
    replacement: Option<(usize, u32, &[u8])>,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&file.header);
    if let Some((_, offset, _)) = replacement {
        bytes[60..64].copy_from_slice(&offset.to_be_bytes());
    }
    for (index, chunk) in file.chunks.iter().enumerate() {
        let data = match replacement {
            Some((map_index, _, data)) if index == map_index => data,
            _ => &chunk.data,
        };
        bytes.extend_from_slice(&chunk.key.kind);
        bytes.extend_from_slice(&((data.len() + 76) as u32).to_be_bytes());
        bytes.extend_from_slice(&chunk.key.id.to_be_bytes());
        bytes.extend_from_slice(&chunk.flags.to_be_bytes());
        bytes.extend_from_slice(&chunk.label);
        bytes.extend_from_slice(data);
    }
    bytes
}

/// Preserves exact source bytes and rebuilds validated resource maps on edits.
#[derive(Clone, Debug)]
pub struct IffDocument {
    file: IffFile,
    original: Vec<u8>,
    indexed: bool,
}

impl IffDocument {
    pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Self> {
        let file = decode(bytes, limits)?;
        let indexed = has_map(&file);
        Ok(Self {
            file,
            original: bytes.to_vec(),
            indexed,
        })
    }
    pub fn file(&self) -> &IffFile {
        &self.file
    }
    pub fn file_mut(&mut self) -> &mut IffFile {
        &mut self.file
    }
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>> {
        encoded_size(&self.file, limits)?;
        if self.matches_original()? {
            limits.check_input(&self.original)?;
            return Ok(self.original.clone());
        }
        if !self.indexed {
            return encode(&self.file, limits);
        }
        encode_rebuilding_index(&self.original, &self.file, limits)
    }

    fn matches_original(&self) -> Result<bool> {
        let mut reader = Reader::new(&self.original);
        if reader.read_bytes(64)? != self.file.header {
            return Ok(false);
        }
        for chunk in &self.file.chunks {
            if reader.remaining() < 76 {
                return Ok(false);
            }
            if reader.read_bytes(4)? != chunk.key.kind
                || reader.u32_be()? as usize != chunk.data.len() + 76
                || reader.u16_be()? != chunk.key.id
                || reader.u16_be()? != chunk.flags
                || reader.read_bytes(64)? != chunk.label
            {
                return Ok(false);
            }
            if reader.read_bytes(chunk.data.len())? != chunk.data {
                return Ok(false);
            }
        }
        Ok(reader.remaining() == 0)
    }
}
