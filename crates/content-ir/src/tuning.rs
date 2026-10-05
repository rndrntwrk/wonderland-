// Source-derived from FreeSO WorldObjectProvider.cs, VMEntityTuning.cs, and
// VMMemory.cs. Mozilla Public License, v. 2.0. https://mozilla.org/MPL/2.0/
//! BCON < selected OTF < upgrade < dynamic private < dynamic semiglobal.
//! Dynamic global replacements are absent in the source and are not invented.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wonderland_legacy_formats::{
    iff::IffFile,
    semantic::{decode_bcon, encode_otf, Otf},
    Error, ErrorKind, Limits, Result,
};

// A deliberately conservative bound per live BTreeMap entry, including the
// minimum leaf allocation and internal-node overhead at these fixed key sizes.
const MAP_ENTRY_BOUND: usize = 512;

fn check_entry_budget(count: usize, limits: &Limits) -> Result<()> {
    limits.check_count(count, limits.max_entries, 0, "total tuning entries")?;
    let bytes = count
        .checked_mul(MAP_ENTRY_BOUND)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "tuning map allocation"))?;
    limits.check_count(
        bytes,
        limits.max_total_decoded_bytes,
        0,
        "tuning map allocation",
    )
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TuningOverride {
    pub table: i32,
    pub index: i32,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DynamicOverride {
    pub table: i32,
    pub index: i32,
    pub value_bits: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TuningInputs {
    pub otf: Option<Otf>,
    /// A selected rewrite replaces the archive OTF, rather than merging it.
    pub otf_rewrite: Option<Otf>,
    pub upgrades: Vec<TuningOverride>,
    /// Table IDs are relative and receive the source's 4096 offset.
    pub dynamic_private: Vec<DynamicOverride>,
    /// Table IDs are relative and receive the source's 8192 offset.
    pub dynamic_semiglobal: Vec<DynamicOverride>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TuningScope {
    Private,
    Semiglobal,
    Global,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TuningOrigin {
    Bcon,
    Otf,
    OtfRewrite,
    Upgrade,
    DynamicPrivate,
    DynamicSemiglobal,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TunedValue {
    pub value: i16,
    pub origin: TuningOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TuningLookup {
    pub scope: TuningScope,
    pub table: u16,
    pub index: u16,
    pub value: i16,
    pub origin: TuningOrigin,
    pub used_private_fallback: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedTuning {
    pub private_cache: BTreeMap<u32, TunedValue>,
    pub semiglobal_cache: BTreeMap<u32, TunedValue>,
    pub global_cache: BTreeMap<u32, TunedValue>,
    pub replacements: BTreeMap<(i32, i32), TunedValue>,
    pub has_semiglobal: bool,
    pub identity: [u8; 32],
}

impl ResolvedTuning {
    pub fn retained_heap_bytes(&self) -> Result<usize> {
        let count = self
            .private_cache
            .len()
            .checked_add(self.semiglobal_cache.len())
            .and_then(|n| n.checked_add(self.global_cache.len()))
            .and_then(|n| n.checked_add(self.replacements.len()))
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "tuning retained entries"))?;
        count
            .checked_mul(MAP_ENTRY_BOUND)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "tuning retained budget"))
    }
    /// VMMemory.GetTuningVariable preserves the source's >=192 mode-zero quirk.
    pub fn lookup_encoded(&self, data: u16) -> TuningLookup {
        let raw = data >> 7;
        let index = data & 127;
        let (scope, table) = if raw < 64 {
            (TuningScope::Private, raw + 4096)
        } else if raw < 128 {
            (TuningScope::Semiglobal, raw - 64 + 8192)
        } else if raw < 192 {
            (TuningScope::Global, raw - 128 + 256)
        } else {
            (TuningScope::Private, raw + 4096)
        };
        let fallback = scope == TuningScope::Semiglobal && !self.has_semiglobal;
        let packed = (u32::from(table) << 16) | u32::from(index);
        let cache = match scope {
            TuningScope::Private => &self.private_cache,
            TuningScope::Semiglobal if fallback => &self.private_cache,
            TuningScope::Semiglobal => &self.semiglobal_cache,
            TuningScope::Global => &self.global_cache,
        };
        let value = self
            .replacements
            .get(&(i32::from(table), i32::from(index)))
            .or_else(|| cache.get(&packed))
            .copied()
            .unwrap_or(TunedValue {
                value: 0,
                origin: TuningOrigin::Missing,
            });
        TuningLookup {
            scope,
            table,
            index,
            value: value.value,
            origin: value.origin,
            used_private_fallback: fallback,
        }
    }
    pub fn value_or_zero(&self, data: u16) -> i16 {
        self.lookup_encoded(data).value
    }
}

fn base_cache(
    file: Option<&IffFile>,
    limits: &Limits,
    total: &mut usize,
) -> Result<BTreeMap<u32, TunedValue>> {
    let mut cache = BTreeMap::new();
    let Some(file) = file else {
        return Ok(cache);
    };
    limits.check_count(
        file.chunks.len(),
        limits.max_entries,
        0,
        "tuning IFF chunks",
    )?;
    for chunk in &file.chunks {
        if chunk.key.kind != *b"BCON" {
            continue;
        }
        let bcon = decode_bcon(&chunk.data, limits)?;
        for (index, value) in bcon.constants.iter().enumerate() {
            *total = total
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "tuning entries"))?;
            check_entry_budget(*total, limits)?;
            cache.insert(
                (u32::from(chunk.key.id) << 16) | index as u32,
                TunedValue {
                    value: *value as i16,
                    origin: TuningOrigin::Bcon,
                },
            );
        }
    }
    Ok(cache)
}

fn dynamic_short(bits: u32) -> Result<i16> {
    let value = f32::from_bits(bits);
    let truncated = value.trunc();
    // C# leaves out-of-range floating-to-integer conversion implementation
    // dependent. Reject it instead of making deterministic simulation platform-dependent.
    if !value.is_finite() || truncated < f32::from(i16::MIN) || truncated > f32::from(i16::MAX) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            0,
            "dynamic tuning float is outside deterministic i16 range",
        ));
    }
    Ok(truncated as i16)
}

fn append_dynamic(
    result: &mut ResolvedTuning,
    values: &[DynamicOverride],
    offset: i32,
    origin: TuningOrigin,
) -> Result<()> {
    for value in values {
        let table = value
            .table
            .checked_add(offset)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "dynamic tuning table offset"))?;
        result.replacements.insert(
            (table, value.index),
            TunedValue {
                value: dynamic_short(value.value_bits)?,
                origin,
            },
        );
    }
    Ok(())
}

pub fn resolve_tuning(
    private: &IffFile,
    semiglobal: Option<&IffFile>,
    global: Option<&IffFile>,
    inputs: &TuningInputs,
    limits: &Limits,
) -> Result<ResolvedTuning> {
    let mut input_entries = inputs
        .upgrades
        .len()
        .checked_add(inputs.dynamic_private.len())
        .and_then(|n| n.checked_add(inputs.dynamic_semiglobal.len()))
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "tuning input count"))?;
    check_entry_budget(input_entries, limits)?;
    let private_cache = base_cache(Some(private), limits, &mut input_entries)?;
    let semiglobal_cache = base_cache(semiglobal, limits, &mut input_entries)?;
    let global_cache = base_cache(global, limits, &mut input_entries)?;
    let mut result = ResolvedTuning {
        private_cache,
        semiglobal_cache,
        global_cache,
        has_semiglobal: semiglobal.is_some(),
        ..ResolvedTuning::default()
    };
    if let Some(otf) = inputs.otf_rewrite.as_ref().or(inputs.otf.as_ref()) {
        encode_otf(otf, limits)?;
        for table in &otf.tables {
            for key in &table.keys {
                input_entries = input_entries
                    .checked_add(1)
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "OTF tuning count"))?;
                check_entry_budget(input_entries, limits)?;
                // Preserve the original uint shifts/OR, including unusual
                // signed or over-16-bit XML IDs, and unchecked int-to-short cast.
                let packed = ((table.id as u32) << 16) | key.id as u32;
                result.private_cache.insert(
                    packed,
                    TunedValue {
                        value: key.value as i16,
                        origin: if inputs.otf_rewrite.is_some() {
                            TuningOrigin::OtfRewrite
                        } else {
                            TuningOrigin::Otf
                        },
                    },
                );
            }
        }
    }
    for value in &inputs.upgrades {
        result.replacements.insert(
            (value.table, value.index),
            TunedValue {
                value: value.value,
                origin: TuningOrigin::Upgrade,
            },
        );
    }
    append_dynamic(
        &mut result,
        &inputs.dynamic_private,
        4096,
        TuningOrigin::DynamicPrivate,
    )?;
    if result.has_semiglobal {
        append_dynamic(
            &mut result,
            &inputs.dynamic_semiglobal,
            8192,
            TuningOrigin::DynamicSemiglobal,
        )?;
    }
    result.identity = tuning_identity(&result, inputs, limits)?;
    Ok(result)
}

fn tuning_identity(
    result: &ResolvedTuning,
    inputs: &TuningInputs,
    limits: &Limits,
) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(b"wonderland.tuning.v1\0");
    for otf in [&inputs.otf, &inputs.otf_rewrite] {
        hash.update([u8::from(otf.is_some())]);
        if let Some(otf) = otf {
            let encoded = encode_otf(otf, limits)?;
            hash.update((encoded.len() as u64).to_le_bytes());
            hash.update(encoded);
        }
    }
    hash.update((inputs.upgrades.len() as u64).to_le_bytes());
    for v in &inputs.upgrades {
        hash.update(v.table.to_le_bytes());
        hash.update(v.index.to_le_bytes());
        hash.update(v.value.to_le_bytes());
    }
    for sequence in [&inputs.dynamic_private, &inputs.dynamic_semiglobal] {
        hash.update((sequence.len() as u64).to_le_bytes());
        for v in sequence {
            hash.update(v.table.to_le_bytes());
            hash.update(v.index.to_le_bytes());
            hash.update(v.value_bits.to_le_bytes());
        }
    }
    hash.update([u8::from(result.has_semiglobal)]);
    for cache in [
        &result.private_cache,
        &result.semiglobal_cache,
        &result.global_cache,
    ] {
        hash.update((cache.len() as u64).to_le_bytes());
        for (key, value) in cache {
            hash.update(key.to_le_bytes());
            hash.update(value.value.to_le_bytes());
            hash.update([value.origin as u8]);
        }
    }
    hash.update((result.replacements.len() as u64).to_le_bytes());
    for ((table, index), value) in &result.replacements {
        hash.update(table.to_le_bytes());
        hash.update(index.to_le_bytes());
        hash.update(value.value.to_le_bytes());
        hash.update([value.origin as u8]);
    }
    Ok(hash.finalize().into())
}
