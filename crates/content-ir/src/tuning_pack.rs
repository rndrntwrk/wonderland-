// SPDX-License-Identifier: MPL-2.0
//! Immutable effective tuning values, including private/semiglobal/global lookup
//! and the exact resolver identity. This is a new pack format, not legacy OTF.
use crate::tuning::{ResolvedTuning, TunedValue, TuningOrigin};
use std::collections::BTreeMap;
use wonderland_legacy_formats::{reader::Reader, Error, ErrorKind, Limits, Result};

pub const MAGIC: &[u8; 8] = b"WLTUNE\0\0";
pub const VERSION: u32 = 1;
pub const HEADER_BYTES: usize = 64;

fn invalid(context: &str) -> Error {
    Error::new(ErrorKind::InvalidData, 0, context)
}
fn budget(counts: [usize; 4], limits: &Limits) -> Result<usize> {
    let total = counts.into_iter().try_fold(0usize, |a, b| {
        a.checked_add(b)
            .ok_or_else(|| invalid("tuning count overflow"))
    })?;
    limits.check_count(total, limits.max_entries, 48, "effective tuning entries")?;
    for count in counts {
        if count > u32::MAX as usize {
            return Err(invalid("tuning count exceeds format width"));
        }
    }
    // Conservative allowance for BTreeMap storage on the pinned native and
    // wasm32 toolchain, checked BEFORE any map is populated. This is a bounded
    // import budget, not a claim to track the process allocator's lifetime.
    let allocation = total
        .checked_mul(256)
        .and_then(|n| n.checked_add(std::mem::size_of::<ResolvedTuning>()))
        .ok_or_else(|| invalid("tuning allocation overflow"))?;
    limits.check_count(
        allocation,
        limits.max_total_decoded_bytes,
        48,
        "effective tuning allocation",
    )?;
    let caches = counts[..3].iter().try_fold(0usize, |a, b| {
        a.checked_add(*b)
            .ok_or_else(|| invalid("tuning count overflow"))
    })?;
    let length = caches
        .checked_mul(8)
        .and_then(|n| counts[3].checked_mul(12).and_then(|r| n.checked_add(r)))
        .and_then(|n| n.checked_add(HEADER_BYTES))
        .ok_or_else(|| invalid("tuning byte size overflow"))?;
    limits.check_count(
        length,
        limits.max_resource_bytes,
        0,
        "effective tuning bytes",
    )?;
    limits.check_count(
        length,
        limits.max_input_bytes,
        0,
        "effective tuning input/output bytes",
    )?;
    Ok(length)
}

fn tag(origin: TuningOrigin) -> Result<u8> {
    match origin {
        TuningOrigin::Bcon => Ok(0),
        TuningOrigin::Otf => Ok(1),
        TuningOrigin::OtfRewrite => Ok(2),
        TuningOrigin::Upgrade => Ok(3),
        TuningOrigin::DynamicPrivate => Ok(4),
        TuningOrigin::DynamicSemiglobal => Ok(5),
        TuningOrigin::Missing => Err(invalid("missing tuning values must remain absent")),
    }
}
fn origin(tag: u8) -> Result<TuningOrigin> {
    match tag {
        0 => Ok(TuningOrigin::Bcon),
        1 => Ok(TuningOrigin::Otf),
        2 => Ok(TuningOrigin::OtfRewrite),
        3 => Ok(TuningOrigin::Upgrade),
        4 => Ok(TuningOrigin::DynamicPrivate),
        5 => Ok(TuningOrigin::DynamicSemiglobal),
        _ => Err(invalid("unknown effective tuning origin")),
    }
}
fn validate_origin(value: TunedValue, section: usize, has_semiglobal: bool) -> Result<()> {
    let allowed = match section {
        0 => matches!(
            value.origin,
            TuningOrigin::Bcon | TuningOrigin::Otf | TuningOrigin::OtfRewrite
        ),
        1 | 2 => value.origin == TuningOrigin::Bcon,
        _ => {
            matches!(
                value.origin,
                TuningOrigin::Upgrade | TuningOrigin::DynamicPrivate
            ) || (has_semiglobal && value.origin == TuningOrigin::DynamicSemiglobal)
        }
    };
    if !allowed {
        return Err(invalid("tuning origin is invalid for its scope"));
    }
    Ok(())
}

/// Serializes source-resolved caches, preserving the resolver's identity. Pack
/// SHA-256 provides byte integrity; this identity records the original inputs.
pub fn encode(value: &ResolvedTuning, limits: &Limits) -> Result<Vec<u8>> {
    let caches = [
        &value.private_cache,
        &value.semiglobal_cache,
        &value.global_cache,
    ];
    let counts = [
        caches[0].len(),
        caches[1].len(),
        caches[2].len(),
        value.replacements.len(),
    ];
    let length = budget(counts, limits)?;
    if !value.has_semiglobal && !value.semiglobal_cache.is_empty() {
        return Err(invalid("semiglobal cache exists without its scope"));
    }
    for (section, cache) in caches.iter().enumerate() {
        for entry in cache.values() {
            validate_origin(*entry, section, value.has_semiglobal)?;
        }
    }
    for entry in value.replacements.values() {
        validate_origin(*entry, 3, value.has_semiglobal)?;
    }
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&[u8::from(value.has_semiglobal), 0, 0, 0]);
    out.extend_from_slice(&value.identity);
    for count in counts {
        out.extend_from_slice(&(count as u32).to_le_bytes());
    }
    for cache in caches {
        for (key, value) in cache {
            out.extend_from_slice(&key.to_le_bytes());
            out.extend_from_slice(&value.value.to_le_bytes());
            out.extend_from_slice(&[tag(value.origin)?, 0]);
        }
    }
    for ((table, index), value) in &value.replacements {
        out.extend_from_slice(&table.to_le_bytes());
        out.extend_from_slice(&index.to_le_bytes());
        out.extend_from_slice(&value.value.to_le_bytes());
        out.extend_from_slice(&[tag(value.origin)?, 0]);
    }
    Ok(out)
}

fn read_value(reader: &mut Reader<'_>, section: usize, has_semiglobal: bool) -> Result<TunedValue> {
    let value = TunedValue {
        value: reader.i16_le()?,
        origin: origin(reader.u8()?)?,
    };
    if reader.u8()? != 0 {
        return Err(invalid("nonzero reserved tuning entry byte"));
    }
    validate_origin(value, section, has_semiglobal)?;
    Ok(value)
}

/// Checks all counts and the complete encoded length before allocating any
/// lookup maps; duplicate or noncanonical keys and reserved flags are rejected.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<ResolvedTuning> {
    limits.check_input(bytes)?;
    let mut reader = Reader::new(bytes);
    if reader.read_bytes(8)? != MAGIC {
        return Err(Error::new(
            ErrorKind::InvalidMagic,
            0,
            "effective tuning magic",
        ));
    }
    if reader.u32_le()? != VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            8,
            "effective tuning version",
        ));
    }
    let has_semiglobal = match reader.u8()? {
        0 => false,
        1 => true,
        _ => return Err(invalid("invalid semiglobal flag")),
    };
    if reader.read_bytes(3)? != [0, 0, 0] {
        return Err(invalid("nonzero reserved tuning flags"));
    }
    let identity = reader
        .read_bytes(32)?
        .try_into()
        .map_err(|_| invalid("tuning identity length"))?;
    let counts = [
        reader.u32_le()? as usize,
        reader.u32_le()? as usize,
        reader.u32_le()? as usize,
        reader.u32_le()? as usize,
    ];
    if budget(counts, limits)? != bytes.len() {
        return Err(invalid("effective tuning length mismatch"));
    }
    if !has_semiglobal && counts[1] != 0 {
        return Err(invalid("semiglobal cache exists without its scope"));
    }
    let mut caches: [BTreeMap<u32, TunedValue>; 3] = std::array::from_fn(|_| BTreeMap::new());
    for section in 0..3 {
        let mut previous = None;
        for _ in 0..counts[section] {
            let key = reader.u32_le()?;
            if previous.is_some_and(|p| key <= p) {
                return Err(invalid("duplicate or unordered tuning key"));
            }
            previous = Some(key);
            let value = read_value(&mut reader, section, has_semiglobal)?;
            caches[section].insert(key, value);
        }
    }
    let mut replacements = BTreeMap::new();
    let mut previous = None;
    for _ in 0..counts[3] {
        let key = (reader.i32_le()?, reader.i32_le()?);
        if previous.is_some_and(|p| key <= p) {
            return Err(invalid("duplicate or unordered replacement key"));
        }
        previous = Some(key);
        let value = read_value(&mut reader, 3, has_semiglobal)?;
        replacements.insert(key, value);
    }
    let [private_cache, semiglobal_cache, global_cache] = caches;
    Ok(ResolvedTuning {
        private_cache,
        semiglobal_cache,
        global_cache,
        replacements,
        has_semiglobal,
        identity,
    })
}
