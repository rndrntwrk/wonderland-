// SPDX-License-Identifier: MPL-2.0
//! RefPack commands and the source-derived 9-byte QFS (`0xFB10`) header.
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};

/// Decodes a raw RefPack command stream. A terminal 0xFC..=0xFF command is
/// required, every input byte must be consumed, and output must match exactly.
pub fn decompress_refpack(bytes: &[u8], decoded_size: usize, limits: &Limits) -> Result<Vec<u8>> {
    limits.check_input(bytes)?;
    limits.check_count(
        decoded_size,
        limits.max_resource_bytes,
        0,
        "RefPack output bytes",
    )?;
    limits.check_count(
        decoded_size,
        limits.max_total_decoded_bytes,
        0,
        "RefPack decoded total",
    )?;
    let mut reader = Reader::new(bytes);
    let mut output = Vec::new();
    output
        .try_reserve_exact(decoded_size)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "RefPack output allocation"))?;
    loop {
        let at = reader.position();
        let command = reader.u8()? as usize;
        let (literal, distance, copy, terminal) = match command {
            0x00..=0x7f => {
                let b = reader.u8()? as usize;
                (
                    command & 3,
                    ((command & 0x60) << 3) + b + 1,
                    ((command & 0x1c) >> 2) + 3,
                    false,
                )
            }
            0x80..=0xbf => {
                let b = reader.u8()? as usize;
                let c = reader.u8()? as usize;
                (
                    (b >> 6) & 3,
                    ((b & 0x3f) << 8) + c + 1,
                    (command & 0x3f) + 4,
                    false,
                )
            }
            0xc0..=0xdf => {
                let b = reader.u8()? as usize;
                let c = reader.u8()? as usize;
                let d = reader.u8()? as usize;
                (
                    command & 3,
                    ((command & 0x10) << 12) + (b << 8) + c + 1,
                    ((command & 0x0c) << 6) + d + 5,
                    false,
                )
            }
            0xe0..=0xfb => (((command & 0x1f) << 2) + 4, 0, 0, false),
            _ => (command & 3, 0, 0, true),
        };
        let expanded = output
            .len()
            .checked_add(literal)
            .and_then(|x| x.checked_add(copy))
            .ok_or_else(|| Error::new(ErrorKind::Overflow, at, "RefPack command output"))?;
        if expanded > decoded_size {
            return Err(Error::new(
                ErrorKind::InvalidData,
                at,
                "RefPack command exceeds declared output",
            ));
        }
        output.extend_from_slice(reader.read_bytes(literal)?);
        if copy != 0 {
            if distance > output.len() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    at,
                    "RefPack backreference precedes output",
                ));
            }
            for _ in 0..copy {
                // Bytewise copying deliberately supports overlapping references.
                let byte = output[output.len() - distance];
                output.push(byte);
            }
        }
        if terminal {
            if reader.remaining() != 0 || output.len() != decoded_size {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    at,
                    "RefPack terminal size or trailing bytes",
                ));
            }
            return Ok(output);
        }
    }
}

/// Decodes the 9-byte QFS header used by the pinned FAR3 reader. Original
/// installed archives count the whole QFS slice; native-compatible vectors can
/// count only commands. Both forms are bounded by the enclosing stored entry.
pub fn decompress_qfs(bytes: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    let (commands, decoded_size) = qfs_header(bytes, limits)?;
    decompress_refpack(commands, decoded_size, limits).map_err(|mut e| {
        e.offset += 9;
        e
    })
}

pub(crate) fn qfs_header<'a>(bytes: &'a [u8], limits: &Limits) -> Result<(&'a [u8], usize)> {
    limits.check_input(bytes)?;
    let mut reader = Reader::new(bytes);
    let stored_size = reader.u32_le()? as usize;
    if reader.u16_le()? != 0xfb10 {
        return Err(Error::new(
            ErrorKind::InvalidMagic,
            4,
            "unsupported QFS signature",
        ));
    }
    let raw = reader.read_bytes(3)?;
    let decoded_size = ((raw[0] as usize) << 16) | ((raw[1] as usize) << 8) | raw[2] as usize;
    if stored_size != reader.remaining() && stored_size != bytes.len() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            0,
            "QFS stored size must equal its command bytes or complete 9-byte-header slice",
        ));
    }
    limits.check_count(
        decoded_size,
        limits.max_resource_bytes,
        6,
        "QFS decoded bytes",
    )?;
    limits.check_count(
        decoded_size,
        limits.max_total_decoded_bytes,
        6,
        "QFS decoded total",
    )?;
    Ok((reader.read_bytes(reader.remaining())?, decoded_size))
}
