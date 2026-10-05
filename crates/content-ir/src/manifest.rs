// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
//! Versioned, immutable asset metadata. This module has no I/O or renderer dependency.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};

pub const MANIFEST_VERSION: u32 = 1;
pub const PACK_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);

impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Digest {
    type Error = ManifestError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ManifestError(
                "digest must be 64 lowercase hexadecimal characters".into(),
            ));
        }
        Ok(Self(value))
    }
}
impl From<Digest> for String {
    fn from(value: Digest) -> String {
        value.0
    }
}
impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError(pub String);
impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ManifestError {}
pub type ManifestResult<T> = Result<T, ManifestError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Semantic,
    Visual,
    Audio,
    Opaque,
}

/// The consumer must use this explicit decoder after pack integrity verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCodec {
    IffChunk,
    IffTtabTsbo,
    ResolvedTuning,
    VitaboyAnimation,
    VitaboySkeleton,
    VitaboyMesh,
    VitaboyBinding,
    VitaboyAppearance,
    VitaboyOutfit,
    PcmWave,
    XaMetadata,
    UtkMetadata,
    HitTrackMetadata,
    HitEventsMetadata,
    HitHsmMetadata,
    HitBytecodeMetadata,
    HitlistVersioned,
    HitlistCounted,
    HitlistPascalRanges,
    Opaque,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Original,
    Derived,
    Authored,
    UserImported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Redistribution {
    Allowed,
    Restricted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub origin: Origin,
    pub source: String,
    pub source_hash: Digest,
    /// Effective patches in application order. This order is semantic.
    pub patch_hashes: Vec<Digest>,
    pub tuning_hash: Option<Digest>,
    pub license: Option<String>,
    pub redistribution: Redistribution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRecord {
    pub content_hash: Digest,
    pub pack: Digest,
    pub kind: ResourceKind,
    pub codec: ResourceCodec,
    pub dependencies: Vec<String>,
    pub simulation_critical: bool,
    pub locale: Option<String>,
    /// Empty means independent of a renderer; otherwise the named variants are available.
    pub variants: BTreeSet<String>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRecord {
    pub byte_len: u64,
    pub format_version: u32,
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifest {
    pub schema_version: u32,
    pub pack_format_version: u32,
    pub source_baseline: String,
    pub content_version: Digest,
    pub tuning_version: Digest,
    #[serde(deserialize_with = "unique_map")]
    pub packs: BTreeMap<Digest, PackRecord>,
    #[serde(deserialize_with = "unique_map")]
    pub resources: BTreeMap<String, ResourceRecord>,
}

#[derive(Clone, Debug)]
pub struct ManifestLimits {
    pub max_manifest_bytes: usize,
    pub max_resources: usize,
    pub max_packs: usize,
    pub max_dependencies: usize,
    pub max_string_bytes: usize,
    pub max_pack_bytes: u64,
    pub max_download_bytes: u64,
}
impl Default for ManifestLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: 8 * 1024 * 1024,
            max_resources: 100_000,
            max_packs: 20_000,
            max_dependencies: 1_000_000,
            max_string_bytes: 1024,
            max_pack_bytes: 128 * 1024 * 1024,
            max_download_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadPhase {
    Simulation,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadPlan {
    pub manifest_hash: Digest,
    pub content_version: Digest,
    /// Dependency-first order, with deterministic tie-breaking.
    pub resources: Vec<String>,
    pub packs: Vec<Digest>,
    pub download_bytes: u64,
}

impl AssetManifest {
    pub fn seal(mut self, limits: &ManifestLimits) -> ManifestResult<Self> {
        self.validate_structure(limits)?;
        self.normalize();
        self.content_version = self.fingerprint(limits)?;
        bounded_json(&self, limits.max_manifest_bytes)?;
        Ok(self)
    }
    pub fn validate(&self, limits: &ManifestLimits) -> ManifestResult<()> {
        self.validate_structure(limits)?;
        if self.content_version != self.fingerprint(limits)? {
            return Err(ManifestError(
                "manifest content version does not match effective metadata".into(),
            ));
        }
        bounded_json(self, limits.max_manifest_bytes)?;
        Ok(())
    }
    pub fn from_json(bytes: &[u8], limits: &ManifestLimits) -> ManifestResult<Self> {
        if bytes.len() > limits.max_manifest_bytes {
            return Err(ManifestError("manifest byte limit exceeded".into()));
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|e| ManifestError(e.to_string()))?;
        value.validate(limits)?;
        Ok(value)
    }
    /// Verify against the manifest digest selected by the release before interpreting it.
    /// The caller obtains that digest from its trusted release descriptor, not this file.
    pub fn from_json_verified(
        bytes: &[u8],
        expected: &Digest,
        limits: &ManifestLimits,
    ) -> ManifestResult<Self> {
        if bytes.len() > limits.max_manifest_bytes || &Digest::of(bytes) != expected {
            return Err(ManifestError(
                "manifest size or release digest mismatch".into(),
            ));
        }
        Self::from_json(bytes, limits)
    }
    pub fn canonical_bytes(&self, limits: &ManifestLimits) -> ManifestResult<Vec<u8>> {
        self.validate(limits)?;
        let mut normalized = self.clone();
        normalized.normalize();
        bounded_json(&normalized, limits.max_manifest_bytes)
    }
    /// `cached` contains hashes of packs already verified with the pack decoder.
    /// Presence in an unverified filesystem cache is not proof of readiness.
    pub fn load_plan(
        &self,
        roots: &[String],
        phase: LoadPhase,
        variant: Option<&str>,
        cached: &BTreeSet<Digest>,
        limits: &ManifestLimits,
    ) -> ManifestResult<LoadPlan> {
        self.validate(limits)?;
        if roots.len() > limits.max_resources || cached.len() > limits.max_packs {
            return Err(ManifestError("load request count limit exceeded".into()));
        }
        let selected = self.closure(roots)?;
        let resources: Vec<String> = self
            .topological_order()?
            .into_iter()
            .filter(|id| {
                selected.contains(id)
                    && (phase == LoadPhase::All || self.resources[id].simulation_critical)
            })
            .collect();
        let mut packs = BTreeSet::new();
        for id in &resources {
            let record = &self.resources[id];
            if !record.variants.is_empty() && !variant.is_some_and(|v| record.variants.contains(v))
            {
                return Err(ManifestError(format!(
                    "resource {id} has no selected compatible variant"
                )));
            }
            if !cached.contains(&record.pack) {
                packs.insert(record.pack.clone());
            }
        }
        let mut download_bytes = 0u64;
        for hash in &packs {
            download_bytes = download_bytes
                .checked_add(self.packs[hash].byte_len)
                .ok_or_else(|| ManifestError("download size overflow".into()))?;
            if download_bytes > limits.max_download_bytes {
                return Err(ManifestError("load plan exceeds download budget".into()));
            }
        }
        Ok(LoadPlan {
            manifest_hash: Digest::of(&self.canonical_bytes(limits)?),
            content_version: self.content_version.clone(),
            resources,
            packs: packs.into_iter().collect(),
            download_bytes,
        })
    }
    /// Enforces recorded rights; it does not adjudicate whether a supplied license is valid.
    pub fn require_public_distribution(&self) -> ManifestResult<()> {
        for (id, record) in &self.resources {
            if record.provenance.redistribution != Redistribution::Allowed
                || record
                    .provenance
                    .license
                    .as_ref()
                    .is_none_or(|v| v.trim().is_empty())
            {
                return Err(ManifestError(format!(
                    "resource {id} lacks explicit redistribution permission and license"
                )));
            }
        }
        Ok(())
    }
    /// Includes source/patch/tuning identities of every dependency, with patch order intact.
    pub fn derived_cache_key(
        &self,
        id: &str,
        converter: &str,
        variant: &str,
        limits: &ManifestLimits,
    ) -> ManifestResult<Digest> {
        self.validate(limits)?;
        validate_id(converter, limits.max_string_bytes)?;
        validate_id(variant, limits.max_string_bytes)?;
        let selected = self.closure(&[id.into()])?;
        let mut normalized = self.clone();
        normalized.normalize();
        let records: BTreeMap<_, _> = selected
            .into_iter()
            .map(|id| {
                let record = normalized
                    .resources
                    .remove(&id)
                    .expect("closure only contains validated resources");
                (id, record)
            })
            .collect();
        let bytes = bounded_json(
            &(
                "wonderland-derived-v1",
                self.schema_version,
                self.pack_format_version,
                &self.source_baseline,
                &self.tuning_version,
                converter,
                variant,
                records,
            ),
            limits.max_manifest_bytes,
        )?;
        Ok(Digest::of(&bytes))
    }

    fn normalize(&mut self) {
        for record in self.resources.values_mut() {
            record.dependencies.sort();
        }
        for pack in self.packs.values_mut() {
            pack.resources.sort();
        }
    }
    fn fingerprint(&self, limits: &ManifestLimits) -> ManifestResult<Digest> {
        // Check serialized size before copying metadata into canonical order.
        bounded_json(self, limits.max_manifest_bytes)?;
        let mut normalized = self.clone();
        normalized.normalize();
        let bytes = bounded_json(
            &(
                "wonderland-content-v1",
                normalized.schema_version,
                normalized.pack_format_version,
                normalized.source_baseline,
                normalized.tuning_version,
                normalized.packs,
                normalized.resources,
            ),
            limits.max_manifest_bytes,
        )?;
        Ok(Digest::of(&bytes))
    }
    fn validate_structure(&self, limits: &ManifestLimits) -> ManifestResult<()> {
        if self.schema_version != MANIFEST_VERSION || self.pack_format_version != PACK_VERSION {
            return Err(ManifestError("unsupported manifest or pack version".into()));
        }
        if ![40, 64].contains(&self.source_baseline.len())
            || !self
                .source_baseline
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ManifestError(
                "source baseline must be a full lowercase git commit hash".into(),
            ));
        }
        if self.resources.len() > limits.max_resources || self.packs.len() > limits.max_packs {
            return Err(ManifestError(
                "manifest resource or pack count limit exceeded".into(),
            ));
        }
        let mut dependencies = 0usize;
        let mut patches = 0usize;
        for (id, resource) in &self.resources {
            validate_id(id, limits.max_string_bytes)?;
            if !self.packs.contains_key(&resource.pack) {
                return Err(ManifestError(format!("missing pack for {id}")));
            }
            if resource.simulation_critical && resource.kind != ResourceKind::Semantic {
                return Err(ManifestError(format!(
                    "tick-critical resource {id} must have semantic kind"
                )));
            }
            if resource.simulation_critical
                && !matches!(
                    resource.codec,
                    ResourceCodec::IffChunk
                        | ResourceCodec::IffTtabTsbo
                        | ResourceCodec::ResolvedTuning
                        | ResourceCodec::VitaboyAnimation
                )
            {
                return Err(ManifestError(format!(
                    "tick-critical resource {id} lacks a supported semantic codec"
                )));
            }
            dependencies = dependencies
                .checked_add(resource.dependencies.len())
                .ok_or_else(|| ManifestError("dependency count overflow".into()))?;
            patches = patches
                .checked_add(resource.provenance.patch_hashes.len())
                .ok_or_else(|| ManifestError("patch count overflow".into()))?;
            if dependencies > limits.max_dependencies || patches > limits.max_dependencies {
                return Err(ManifestError(
                    "dependency or patch count limit exceeded".into(),
                ));
            }
            let mut seen = BTreeSet::new();
            for dependency in &resource.dependencies {
                if !seen.insert(dependency) {
                    return Err(ManifestError(format!("duplicate dependency in {id}")));
                }
                let other = self.resources.get(dependency).ok_or_else(|| {
                    ManifestError(format!("missing dependency {dependency} required by {id}"))
                })?;
                if resource.simulation_critical && !other.simulation_critical {
                    return Err(ManifestError(format!("tick-critical dependency {dependency} is not declared ready before simulation")));
                }
            }
            check_text(
                &resource.provenance.source,
                limits.max_string_bytes,
                "provenance source",
            )?;
            if let Some(license) = &resource.provenance.license {
                check_text(license, limits.max_string_bytes, "license")?;
            }
            if let Some(locale) = &resource.locale {
                validate_id(locale, 64.min(limits.max_string_bytes))?;
            }
            if resource.variants.len() > 32 {
                return Err(ManifestError("variant count limit exceeded".into()));
            }
            for variant in &resource.variants {
                validate_id(variant, 64.min(limits.max_string_bytes))?;
            }
        }
        let mut indexed = BTreeSet::new();
        for (hash, pack) in &self.packs {
            if pack.format_version != PACK_VERSION
                || pack.byte_len < 32
                || pack.byte_len > limits.max_pack_bytes
            {
                return Err(ManifestError(format!(
                    "invalid pack version or size for {hash}"
                )));
            }
            if pack.resources.is_empty() || pack.resources.len() > limits.max_resources {
                return Err(ManifestError(
                    "empty or oversized pack resource index".into(),
                ));
            }
            for id in &pack.resources {
                let resource = self.resources.get(id).ok_or_else(|| {
                    ManifestError(format!("pack references unknown resource {id}"))
                })?;
                if &resource.pack != hash || !indexed.insert(id) {
                    return Err(ManifestError(format!(
                        "duplicate or mismatched pack ownership for {id}"
                    )));
                }
                if indexed.len() > limits.max_resources {
                    return Err(ManifestError("pack index count limit exceeded".into()));
                }
            }
        }
        if indexed.len() != self.resources.len() {
            return Err(ManifestError(
                "pack indexes do not cover all resources".into(),
            ));
        }
        self.topological_order()?;
        Ok(())
    }
    fn closure(&self, roots: &[String]) -> ManifestResult<BTreeSet<String>> {
        let mut result = BTreeSet::new();
        let mut pending: Vec<&str> = roots.iter().map(String::as_str).collect();
        while let Some(id) = pending.pop() {
            if result.contains(id) {
                continue;
            }
            let resource = self
                .resources
                .get(id)
                .ok_or_else(|| ManifestError(format!("unknown resource {id}")))?;
            result.insert(id.to_string());
            pending.extend(resource.dependencies.iter().map(String::as_str));
        }
        Ok(result)
    }
    fn topological_order(&self) -> ManifestResult<Vec<String>> {
        // Iterative Kahn traversal avoids input-controlled recursion depth on WASM.
        let mut counts: BTreeMap<&str, usize> = self
            .resources
            .iter()
            .map(|(id, r)| (id.as_str(), r.dependencies.len()))
            .collect();
        let mut consumers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (id, record) in &self.resources {
            for dependency in &record.dependencies {
                if !self.resources.contains_key(dependency) {
                    return Err(ManifestError(format!("unknown dependency {dependency}")));
                }
                consumers.entry(dependency).or_default().push(id);
            }
        }
        let mut ready: BTreeSet<&str> = counts
            .iter()
            .filter_map(|(id, n)| (*n == 0).then_some(*id))
            .collect();
        let mut output = Vec::with_capacity(self.resources.len());
        while let Some(id) = ready.pop_first() {
            output.push(id.to_string());
            if let Some(next) = consumers.get(id) {
                for consumer in next {
                    let count = counts
                        .get_mut(consumer)
                        .ok_or_else(|| ManifestError("invalid graph".into()))?;
                    *count = count
                        .checked_sub(1)
                        .ok_or_else(|| ManifestError("invalid dependency multiplicity".into()))?;
                    if *count == 0 {
                        ready.insert(consumer);
                    }
                }
            }
        }
        if output.len() != self.resources.len() {
            return Err(ManifestError("resource dependency cycle".into()));
        }
        Ok(output)
    }
}

/// Logical resource IDs are portable identifiers, never arbitrary filesystem paths.
pub fn validate_id(value: &str, max: usize) -> ManifestResult<()> {
    if value.is_empty()
        || value.len() > max
        || value.starts_with('/')
        || value.ends_with('/')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_@.+:-".contains(&b))
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ManifestError(
            "invalid or oversized logical resource identifier".into(),
        ));
    }
    Ok(())
}
fn check_text(value: &str, max: usize, name: &str) -> ManifestResult<()> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ManifestError(format!("invalid or oversized {name}")));
    }
    Ok(())
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let size = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("metadata size overflow"))?;
        if size > self.limit {
            return Err(io::Error::other("metadata byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn bounded_json<T: Serialize>(value: &T, limit: usize) -> ManifestResult<Vec<u8>> {
    let mut writer = BoundedBuffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| ManifestError(e.to_string()))?;
    Ok(writer.bytes)
}

fn unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Unique<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K, V> serde::de::Visitor<'de> for Unique<K, V>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map with unique keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = access.next_entry()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate manifest key"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}
