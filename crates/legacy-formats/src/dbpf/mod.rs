// SPDX-License-Identifier: MPL-2.0
//! Bounded DBPF fixed-record indexes; compact indexes are unsupported.
use crate::{
    checked_range, finish_index, reader::Reader, Compression, ContainerEntry, ContainerFormat,
    ContainerIndex, Error, ErrorKind, Limits, ResourceKey, Result,
};

/// Conventional DBPF 1.0 with index 7.0, 32-bit TGI and raw entry bytes.
pub fn index<'a>(bytes: &'a [u8], limits: &Limits) -> Result<ContainerIndex<'a>> {
    read_index(bytes, limits, false)
}

/// Exactly reproduces the pinned `DBPFFile.Read` header offsets for versions
/// 1.0 (96 bytes), 1.1 (88 bytes) and 2.0 (76 bytes), with fixed 20-byte TGI32
/// records. This is a named FreeSO source compatibility layout, not general
/// DBPF 1.1/2.0 support. Compact indexes, TGI64 and compression directories are
/// not interpreted; extraction always returns the bounded stored entry bytes.
pub fn index_source_compatible<'a>(bytes: &'a [u8], limits: &Limits) -> Result<ContainerIndex<'a>> {
    read_index(bytes, limits, true)
}

fn read_index<'a>(
    bytes: &'a [u8],
    limits: &Limits,
    source_compatible: bool,
) -> Result<ContainerIndex<'a>> {
    limits.check_input(bytes)?;
    let mut reader = Reader::new(bytes);
    if reader.read_bytes(4)? != b"DBPF" {
        return Err(Error::new(ErrorKind::InvalidMagic, 0, "DBPF signature"));
    }
    let major = reader.u32_le()?;
    let minor = reader.u32_le()?;
    if (major, minor) != (1, 0) && !(source_compatible && matches!((major, minor), (1, 1) | (2, 0)))
    {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            4,
            "unsupported DBPF header version",
        ));
    }
    reader.skip(12)?;
    if (major, minor) == (1, 0) {
        reader.skip(8)?;
    }
    if major < 2 && reader.u32_le()? != 7 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            reader.position() - 4,
            "DBPF index major version must be 7",
        ));
    }
    let count_at = reader.position();
    let count = reader.u32_le()? as usize;
    limits.check_count(count, limits.max_entries, count_at, "DBPF entry count")?;
    let mut index_offset = if major < 2 {
        reader.u32_le()? as usize
    } else {
        0
    };
    let index_size = reader.u32_le()? as usize;
    if major < 2 {
        let trash_count = reader.u32_le()?;
        let trash_offset = reader.u32_le()?;
        let trash_size = reader.u32_le()?;
        if trash_count != 0 || trash_offset != 0 || trash_size != 0 {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                reader.position() - 12,
                "DBPF hole tables are unsupported",
            ));
        }
    }
    if reader.u32_le()? != 0 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            reader.position() - 4,
            "DBPF fixed-record index minor version must be 0",
        ));
    }
    if major == 2 {
        index_offset = reader.u32_le()? as usize;
        reader.skip(4)?;
    }
    reader.skip(32)?;
    let header_len = reader.position();
    let expected_size = count
        .checked_mul(20)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, count_at, "DBPF index size"))?;
    if index_size != expected_size {
        return Err(Error::new(
            ErrorKind::InvalidData,
            count_at,
            "DBPF fixed-record index size differs from count times 20",
        ));
    }
    if count == 0 && index_offset == 0 {
        return finish_index(
            bytes,
            ContainerFormat::Dbpf,
            Vec::new(),
            header_len,
            0..0,
            limits,
        );
    }
    if index_offset < header_len {
        return Err(Error::new(
            ErrorKind::Overlap,
            index_offset,
            "DBPF index overlaps header",
        ));
    }
    let index_range = checked_range(index_offset, index_size, bytes.len(), "DBPF index range")?;
    reader.seek(index_offset)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let type_id = reader.u32_le()?;
        let group_id = reader.u32_le()?;
        let instance_id = reader.u32_le()?;
        let offset = reader.u32_le()? as usize;
        let size = reader.u32_le()? as usize;
        entries.push(ContainerEntry {
            key: ResourceKey::Dbpf {
                type_id,
                group_id,
                instance_id,
            },
            name: None,
            offset,
            stored_len: size,
            decoded_len: size,
            compression: Compression::None,
        });
    }
    finish_index(
        bytes,
        ContainerFormat::Dbpf,
        entries,
        header_len,
        index_range,
        limits,
    )
}
