//! Disposable derivations with per-category byte budgets and owned handle lifetimes.
use crate::{AssetKey, ViewMode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DerivedKey(pub [u8; 32]);
impl DerivedKey {
    /// `effective_source` hashes the actual patched bytes; `content` identifies the patch set.
    pub fn new(
        effective_source: AssetKey,
        content: AssetKey,
        algorithm_version: u32,
        parameters: &[u8],
    ) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"wonderland-derived-v1\0");
        hash.update(effective_source.0);
        hash.update(content.0);
        hash.update(algorithm_version.to_le_bytes());
        hash.update((parameters.len() as u64).to_le_bytes());
        hash.update(parameters);
        Self(hash.finalize().into())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ThumbnailKey(pub [u8; 32]);
impl ThumbnailKey {
    /// Parameters include camera, appearance/pose, lighting and any effective overrides.
    pub fn new(
        source: DerivedKey,
        mode: ViewMode,
        width: u32,
        height: u32,
        parameters: &[u8],
    ) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"wonderland-thumbnail-v1\0");
        hash.update(source.0);
        hash.update([match mode {
            ViewMode::Full2D => 0,
            ViewMode::Hybrid2D => 1,
            ViewMode::Full3D => 2,
        }]);
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update((parameters.len() as u64).to_le_bytes());
        hash.update(parameters);
        Self(hash.finalize().into())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBytes {
    pub encoded: u64,
    pub decoded_cpu: u64,
    pub staging: u64,
    pub gpu: u64,
}
impl ResourceBytes {
    pub const ZERO: Self = Self {
        encoded: 0,
        decoded_cpu: 0,
        staging: 0,
        gpu: 0,
    };
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            encoded: self.encoded.checked_add(rhs.encoded)?,
            decoded_cpu: self.decoded_cpu.checked_add(rhs.decoded_cpu)?,
            staging: self.staging.checked_add(rhs.staging)?,
            gpu: self.gpu.checked_add(rhs.gpu)?,
        })
    }
    pub fn total(self) -> Option<u64> {
        self.encoded
            .checked_add(self.decoded_cpu)?
            .checked_add(self.staging)?
            .checked_add(self.gpu)
    }
    fn subtract(self, rhs: Self) -> Self {
        Self {
            encoded: self.encoded - rhs.encoded,
            decoded_cpu: self.decoded_cpu - rhs.decoded_cpu,
            staging: self.staging - rhs.staging,
            gpu: self.gpu - rhs.gpu,
        }
    }
    fn fits(self, budget: Self) -> bool {
        self.encoded <= budget.encoded
            && self.decoded_cpu <= budget.decoded_cpu
            && self.staging <= budget.staging
            && self.gpu <= budget.gpu
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheError {
    Oversized,
    AccountingOverflow,
    Pinned,
    Capacity,
}
impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CacheError {}
struct Entry<T> {
    value: T,
    bytes: ResourceBytes,
    last_used: u128,
    pins: u32,
}
pub struct BoundedCache<T> {
    entries: BTreeMap<DerivedKey, Entry<T>>,
    budget: ResourceBytes,
    max_entries: usize,
    resident: ResourceBytes,
    clock: u128,
}
impl<T> BoundedCache<T> {
    pub fn new(budget: ResourceBytes, max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            budget,
            max_entries,
            resident: ResourceBytes::ZERO,
            clock: 0,
        }
    }
    pub fn resident_bytes(&self) -> ResourceBytes {
        self.resident
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&mut self, key: &DerivedKey) -> Option<&T> {
        self.clock = self.clock.saturating_add(1);
        let e = self.entries.get_mut(key)?;
        e.last_used = self.clock;
        Some(&e.value)
    }
    pub fn pin(&mut self, key: &DerivedKey) -> bool {
        let Some(e) = self.entries.get_mut(key) else {
            return false;
        };
        let Some(pins) = e.pins.checked_add(1) else {
            return false;
        };
        e.pins = pins;
        true
    }
    pub fn unpin(&mut self, key: &DerivedKey) -> bool {
        let Some(e) = self.entries.get_mut(key) else {
            return false;
        };
        if e.pins == 0 {
            return false;
        }
        e.pins -= 1;
        true
    }
    /// Preflight eviction atomically: failed inserts retain all existing entries.
    pub fn insert(
        &mut self,
        key: DerivedKey,
        value: T,
        bytes: ResourceBytes,
    ) -> Result<(), CacheError> {
        if bytes.total().is_none() || self.budget.total().is_none() {
            return Err(CacheError::AccountingOverflow);
        }
        if !bytes.fits(self.budget) {
            return Err(CacheError::Oversized);
        }
        if self.max_entries == 0 {
            return Err(CacheError::Capacity);
        }
        if self.entries.get(&key).is_some_and(|e| e.pins > 0) {
            return Err(CacheError::Pinned);
        }
        let old = self
            .entries
            .get(&key)
            .map(|e| e.bytes)
            .unwrap_or(ResourceBytes::ZERO);
        let base = self.resident.subtract(old);
        let mut projected = base
            .checked_add(bytes)
            .ok_or(CacheError::AccountingOverflow)?;
        if projected.total().is_none() {
            return Err(CacheError::AccountingOverflow);
        }
        let mut count = self.entries.len() + usize::from(!self.entries.contains_key(&key));
        let mut candidates: Vec<_> = self
            .entries
            .iter()
            .filter(|(k, e)| **k != key && e.pins == 0)
            .map(|(&k, e)| (e.last_used, k, e.bytes))
            .collect();
        candidates.sort_by_key(|&(age, k, _)| (age, k));
        let mut evictions = Vec::new();
        for (_, k, cost) in candidates {
            if projected.fits(self.budget) && count <= self.max_entries {
                break;
            }
            projected = projected.subtract(cost);
            count -= 1;
            evictions.push(k);
        }
        if !projected.fits(self.budget) || count > self.max_entries {
            return Err(CacheError::Pinned);
        }
        for k in evictions {
            self.remove(&k);
        }
        self.remove(&key);
        self.clock = self.clock.saturating_add(1);
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                last_used: self.clock,
                pins: 0,
            },
        );
        self.resident = projected;
        Ok(())
    }
    /// Pinned entries cannot be explicitly removed; clear is a reset boundary.
    pub fn remove(&mut self, key: &DerivedKey) -> Option<T> {
        if self.entries.get(key)?.pins > 0 {
            return None;
        }
        let e = self.entries.remove(key)?;
        self.resident = self.resident.subtract(e.bytes);
        Some(e.value)
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.resident = ResourceBytes::ZERO;
    }
}
#[derive(Debug)]
pub struct CachedResource<D, G> {
    pub decoded: D,
    pub gpu: Option<G>,
}
impl<D, G> CachedResource<D, G> {
    pub fn new(decoded: D, gpu: Option<G>) -> Self {
        Self { decoded, gpu }
    }
}
impl<D, G> BoundedCache<CachedResource<D, G>> {
    /// Drops GPU handles (including pinned entries), retaining immutable CPU payloads.
    /// Staging data must be owned by the GPU handle or absent in this resource shape.
    pub fn device_reset(&mut self) {
        for e in self.entries.values_mut() {
            e.value.gpu = None;
            e.bytes.gpu = 0;
            e.bytes.staging = 0;
        }
        self.resident.gpu = 0;
        self.resident.staging = 0;
    }
}
