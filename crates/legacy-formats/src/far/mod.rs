// SPDX-License-Identifier: MPL-2.0
//! Explicit FAR1a, FAR1b and FAR3 layouts from the pinned FreeSO source.
use crate::{
    checked_range, compression, finish_index, reader::Reader, Compression, ContainerEntry,
    ContainerFormat, ContainerIndex, Error, ErrorKind, Limits, ResourceKey, Result,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Far1Variant {
    A,
    B,
}

fn manifest<'a>(
    bytes: &'a [u8],
    version: u32,
    limits: &Limits,
) -> Result<(Reader<'a>, usize, usize)> {
    limits.check_input(bytes)?;
    let mut reader = Reader::new(bytes);
    if reader.read_bytes(8)? != b"FAR!byAZ" {
        return Err(Error::new(
            ErrorKind::InvalidMagic,
            0,
            "FAR archive signature",
        ));
    }
    if reader.u32_le()? != version {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            8,
            "FAR version differs from selected layout",
        ));
    }
    let offset = reader.u32_le()? as usize;
    if offset < 16 {
        return Err(Error::new(
            ErrorKind::Overlap,
            12,
            "FAR manifest overlaps header",
        ));
    }
    reader.seek(offset)?;
    let count = reader.u32_le()? as usize;
    limits.check_count(count, limits.max_entries, offset, "FAR entry count")?;
    Ok((reader, offset, count))
}

fn minimum_records(reader: &Reader<'_>, count: usize, width: usize) -> Result<()> {
    let minimum = count
        .checked_mul(width)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, reader.position(), "FAR manifest size"))?;
    if minimum > reader.remaining() {
        return Err(Error::new(
            ErrorKind::Truncated,
            reader.position(),
            "FAR manifest records",
        ));
    }
    Ok(())
}

/// Reads uncompressed FAR1a (u32 filename length) or FAR1b (u16 filename
/// length). FAR1 compressed-size mismatches have no supported source codec.
pub fn index_v1<'a>(
    bytes: &'a [u8],
    variant: Far1Variant,
    limits: &Limits,
) -> Result<ContainerIndex<'a>> {
    let (mut reader, offset, count) = manifest(bytes, 1, limits)?;
    minimum_records(
        &reader,
        count,
        if variant == Far1Variant::A { 16 } else { 14 },
    )?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let at = reader.position();
        let decoded_len = reader.u32_le()? as usize;
        let stored_len = reader.u32_le()? as usize;
        let data_offset = reader.u32_le()? as usize;
        let name_len = if variant == Far1Variant::A {
            reader.u32_le()? as usize
        } else {
            reader.u16_le()? as usize
        };
        limits.check_count(
            name_len,
            limits.max_string_bytes,
            reader.position(),
            "FAR1 filename bytes",
        )?;
        if stored_len != decoded_len {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                at + 4,
                "compressed FAR1 entries are unsupported",
            ));
        }
        let name = reader.read_bytes(name_len)?.to_vec();
        entries.push(ContainerEntry {
            key: ResourceKey::Far1 { name },
            name: None,
            offset: data_offset,
            stored_len,
            decoded_len,
            compression: Compression::None,
        });
    }
    let format = if variant == Far1Variant::A {
        ContainerFormat::Far1a
    } else {
        ContainerFormat::Far1b
    };
    finish_index(
        bytes,
        format,
        entries,
        16,
        offset..reader.position(),
        limits,
    )
}

/// Reads raw FAR3 resources and QFS-compressed Persist resources. The nine-byte
/// Persist prefix is opaque; malformed/unsupported compression framing fails.
pub fn index_v3<'a>(bytes: &'a [u8], limits: &Limits) -> Result<ContainerIndex<'a>> {
    let (mut reader, offset, count) = manifest(bytes, 3, limits)?;
    minimum_records(&reader, count, 24)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let at = reader.position();
        let decoded_len = reader.u32_le()? as usize;
        let raw = reader.read_bytes(3)?;
        let compressed_len = raw[0] as usize | ((raw[1] as usize) << 8) | ((raw[2] as usize) << 16);
        let _data_type = reader.u8()?;
        let data_offset = reader.u32_le()? as usize;
        let compressed = reader.u8()?;
        let _access_number = reader.u8()?;
        let name_len = reader.u16_le()? as usize;
        let type_id = reader.u32_le()?;
        let file_id = reader.u32_le()?;
        limits.check_count(
            name_len,
            limits.max_string_bytes,
            reader.position(),
            "FAR3 filename bytes",
        )?;
        let name = reader.read_bytes(name_len)?.to_vec();
        // Native FAR3Archive.GetEntry ignores DataType. Even raw original
        // bindings set IsCompressed=1, so inspect the bounded Persist signature
        // before choosing QFS; a non-QFS entry is read from its original start.
        let compression = if compressed == 1 {
            limits.check_count(
                compressed_len,
                limits.max_resource_bytes,
                at + 4,
                "FAR3 declared stored bytes",
            )?;
            let range = checked_range(
                data_offset,
                compressed_len,
                bytes.len(),
                "FAR3 declared stored range",
            )?;
            if bytes[range].get(13..15) == Some(&[0x10, 0xfb]) {
                Compression::Far3PersistQfs
            } else {
                Compression::None
            }
        } else {
            Compression::None
        };
        let stored_len = match compression {
            Compression::None => decoded_len,
            Compression::Far3PersistQfs => compressed_len,
        };
        limits.check_count(
            decoded_len,
            limits.max_resource_bytes,
            at,
            "FAR3 decoded bytes",
        )?;
        limits.check_count(
            stored_len,
            limits.max_resource_bytes,
            at + 4,
            "FAR3 stored bytes",
        )?;
        entries.push(ContainerEntry {
            key: ResourceKey::Far3 { type_id, file_id },
            name: Some(name),
            offset: data_offset,
            stored_len,
            decoded_len,
            compression,
        });
    }
    // Check disjoint ranges before validating or decoding compressed payloads.
    let index = finish_index(
        bytes,
        ContainerFormat::Far3,
        entries,
        16,
        offset..reader.position(),
        limits,
    )?;
    for entry in index.entries() {
        if entry.compression == Compression::Far3PersistQfs {
            let range = checked_range(
                entry.offset,
                entry.stored_len,
                bytes.len(),
                "FAR3 Persist entry",
            )?;
            let stored = &bytes[range];
            let qfs = stored.get(9..).ok_or_else(|| {
                Error::new(ErrorKind::Truncated, entry.offset, "FAR3 Persist prefix")
            })?;
            let (_, decoded_size) = compression::qfs_header(qfs, limits).map_err(|mut e| {
                e.offset += entry.offset + 9;
                e
            })?;
            if decoded_size != entry.decoded_len {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    entry.offset + 15,
                    "FAR3 QFS output size differs from manifest",
                ));
            }
        }
    }
    Ok(index)
}
