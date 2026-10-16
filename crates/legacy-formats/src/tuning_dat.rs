// SPDX-License-Identifier: MPL-2.0
//! Bounded TSO tuning.dat reader, following tso.files/Tuning.cs at the pinned source.
use crate::{compression::decompress_qfs, reader::Reader, Error, ErrorKind, Limits, Result};
use std::collections::BTreeMap;

pub type TuningEntries = BTreeMap<String, BTreeMap<String, String>>;

/// The outer persist header precedes the existing nine-byte QFS envelope.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<TuningEntries> {
    limits.check_input(bytes)?;
    let mut input = Reader::new(bytes);
    if !matches!(input.u8()?, 1 | 3) {
        return Err(invalid(0, "unsupported tuning persist body type"));
    }
    let decoded_size = input.u32_le()? as usize;
    let compressed_size = input.u32_le()? as usize;
    if compressed_size != input.remaining() {
        return Err(invalid(5, "tuning persist compressed size mismatch"));
    }
    let decoded = decompress_qfs(input.read_bytes(input.remaining())?, limits)?;
    if decoded.len() != decoded_size {
        return Err(invalid(1, "tuning persist decoded size mismatch"));
    }
    let mut input = Reader::new(&decoded);
    let count = input.u32_le()? as usize;
    limits.check_count(count, limits.max_entries, 0, "tuning entries")?;
    let mut entries = BTreeMap::new();
    let mut total = count;
    // Account for both decoded bytes and owned tree/string allocations.
    let mut allocated = decoded.len();
    for _ in 0..count {
        reserve(&mut allocated, 192, limits)?;
        let name = string(&mut input, limits, &mut allocated)?.to_ascii_lowercase();
        let count = input.u32_le()? as usize;
        total = total
            .checked_add(count)
            .ok_or_else(|| invalid(input.position(), "tuning count overflow"))?;
        limits.check_count(
            total,
            limits.max_entries,
            input.position(),
            "tuning total entries",
        )?;
        let mut values = BTreeMap::new();
        for _ in 0..count {
            reserve(&mut allocated, 128, limits)?;
            let key = string(&mut input, limits, &mut allocated)?;
            let value = string(&mut input, limits, &mut allocated)?;
            if values.insert(key, value).is_some() {
                return Err(invalid(input.position(), "duplicate tuning key"));
            }
        }
        if entries.insert(name, values).is_some() {
            return Err(invalid(input.position(), "duplicate tuning entry name"));
        }
    }
    if input.remaining() != 0 {
        return Err(invalid(input.position(), "trailing tuning payload"));
    }
    Ok(entries)
}

fn invalid(at: usize, reason: &str) -> Error {
    Error::new(ErrorKind::InvalidData, at, reason)
}
fn reserve(total: &mut usize, amount: usize, limits: &Limits) -> Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| invalid(0, "tuning allocation overflow"))?;
    limits.check_count(
        *total,
        limits.max_total_decoded_bytes,
        0,
        "tuning allocation budget",
    )
}
fn string(input: &mut Reader<'_>, limits: &Limits, allocated: &mut usize) -> Result<String> {
    let at = input.position();
    let mut length = 0_u32;
    let mut complete = false;
    for shift in [0, 7, 14, 21, 28] {
        let byte = input.u8()?;
        if shift == 28 && byte > 7 {
            return Err(invalid(at, "tuning string length overflow"));
        }
        length |= u32::from(byte & 127) << shift;
        if byte & 128 == 0 {
            complete = true;
            break;
        }
    }
    if !complete {
        return Err(invalid(at, "unterminated tuning string length"));
    }
    let length = length as usize;
    limits.check_count(length, limits.max_string_bytes, at, "tuning string bytes")?;
    reserve(allocated, length, limits)?;
    let bytes = input.read_bytes(length)?;
    // Encoding.ASCII maps out-of-range decoded bytes to '?'.
    Ok(bytes
        .iter()
        .map(|b| {
            let b = b.wrapping_sub(13);
            if b.is_ascii() {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect())
}
