// SPDX-License-Identifier: MPL-2.0
//! Explicit local import contract. Dependency completeness remains a caller declaration.
use crate::manifest::CookLimits;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use wonderland_content_ir::manifest::{
    validate_id, Digest, ManifestError, ManifestResult, Origin, Redistribution,
};
pub const SOURCE_BASELINE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Iff,
    Far1a,
    Far1b,
    Far3,
    Dbpf,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rights {
    pub origin: Origin,
    pub license: Option<String>,
    pub redistribution: Redistribution,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchSpec {
    pub path: String,
    pub is_user: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSpec {
    pub path: String,
    pub source_name: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TuningSpec {
    #[serde(default)]
    pub otf: Option<String>,
    #[serde(default)]
    pub otf_rewrite: Option<String>,
    #[serde(default)]
    pub upgrades: Vec<UpgradeSpec>,
    #[serde(default)]
    pub dynamic_private: Vec<DynamicSpec>,
    #[serde(default)]
    pub dynamic_semiglobal: Vec<DynamicSpec>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeSpec {
    pub table: i32,
    pub index: i32,
    pub value: i16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicSpec {
    pub table: i32,
    pub index: i32,
    pub value_bits: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleSpec {
    pub requested: u8,
    pub default_language: u8,
}
impl Default for LocaleSpec {
    fn default() -> Self {
        Self {
            requested: 0,
            default_language: 1,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub id: String,
    pub path: String,
    pub format: SourceFormat,
    #[serde(default)]
    pub source_name: Option<String>,
    pub pack_group: String,
    pub provenance: Rights,
    #[serde(default)]
    pub patches: Vec<PatchSpec>,
    #[serde(default)]
    pub semiglobal: Option<ScopeSpec>,
    #[serde(default)]
    pub global: Option<ScopeSpec>,
    #[serde(default)]
    pub tuning: TuningSpec,
    #[serde(default)]
    pub resolver_variant: String,
    #[serde(default)]
    pub locale_selection: LocaleSpec,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceOverride {
    pub id: String,
    pub dependencies: Vec<String>,
    pub simulation_critical: bool,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub variants: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookSpec {
    pub schema_version: u32,
    #[serde(default)]
    pub fixture_only: bool,
    pub source_baseline: String,
    pub tuning_version: Digest,
    pub sources: Vec<SourceSpec>,
    pub overrides: Vec<ResourceOverride>,
}
impl CookSpec {
    pub fn from_json(bytes: &[u8], limits: &CookLimits) -> ManifestResult<Self> {
        if bytes.len() > limits.manifest.max_manifest_bytes {
            return Err(ManifestError("cook spec byte limit exceeded".into()));
        }
        let result: Self =
            serde_json::from_slice(bytes).map_err(|e| ManifestError(e.to_string()))?;
        result.validate(limits)?;
        Ok(result)
    }
    pub fn validate(&self, limits: &CookLimits) -> ManifestResult<()> {
        if self.schema_version != 1 || self.source_baseline != SOURCE_BASELINE {
            return Err(ManifestError(
                "unsupported cook schema or unpinned source baseline".into(),
            ));
        }
        if self.sources.is_empty()
            || self.sources.len() > limits.legacy.max_entries
            || self.overrides.len() > limits.manifest.max_resources
        {
            return Err(ManifestError("source/override count limit exceeded".into()));
        }
        let mut sources = BTreeSet::new();
        let mut overrides = BTreeSet::new();
        let mut patches = 0usize;
        let mut dependencies = 0usize;
        for s in &self.sources {
            validate_id(&s.id, limits.pack.max_id_bytes)?;
            validate_id(&s.pack_group, limits.manifest.max_string_bytes)?;
            if !sources.insert(&s.id) {
                return Err(ManifestError("duplicate source ID".into()));
            }
            crate::fs_io::validate_relative(&s.path)?;
            if s.format != SourceFormat::Iff && !s.patches.is_empty() {
                return Err(ManifestError(
                    "patches require an explicitly named IFF source".into(),
                ));
            }
            for scope in s.semiglobal.iter().chain(s.global.iter()) {
                crate::fs_io::validate_relative(&scope.path)?;
                if scope.source_name.is_empty()
                    || scope.source_name.len() > limits.manifest.max_string_bytes
                    || scope.source_name.chars().any(char::is_control)
                {
                    return Err(ManifestError("invalid scope source name".into()));
                }
            }
            for path in s.tuning.otf.iter().chain(s.tuning.otf_rewrite.iter()) {
                crate::fs_io::validate_relative(path)?;
            }
            let tuning_count = s
                .tuning
                .upgrades
                .len()
                .checked_add(s.tuning.dynamic_private.len())
                .and_then(|n| n.checked_add(s.tuning.dynamic_semiglobal.len()))
                .ok_or_else(|| ManifestError("tuning input overflow".into()))?;
            if tuning_count > limits.legacy.max_entries
                || !matches!(s.resolver_variant.as_str(), "" | "tsbo")
                || s.locale_selection.default_language == 0
            {
                return Err(ManifestError(
                    "invalid tuning, variant or locale input".into(),
                ));
            }
            if s.format != SourceFormat::Iff
                && (s.semiglobal.is_some()
                    || s.global.is_some()
                    || s.tuning.otf.is_some()
                    || s.tuning.otf_rewrite.is_some()
                    || tuning_count != 0
                    || !s.resolver_variant.is_empty())
            {
                return Err(ManifestError(
                    "scope/tuning/resolver variant inputs require an explicit IFF source".into(),
                ));
            }
            for p in &s.patches {
                crate::fs_io::validate_relative(&p.path)?;
            }
            patches = patches
                .checked_add(s.patches.len())
                .ok_or_else(|| ManifestError("patch count overflow".into()))?;
            if patches > limits.legacy.max_entries {
                return Err(ManifestError("patch count limit exceeded".into()));
            }
            for text in s.source_name.iter().chain(s.provenance.license.iter()) {
                if text.is_empty()
                    || text.len() > limits.manifest.max_string_bytes
                    || text.chars().any(char::is_control)
                {
                    return Err(ManifestError("invalid source name/license".into()));
                }
            }
        }
        for o in &self.overrides {
            validate_id(&o.id, limits.pack.max_id_bytes)?;
            if !overrides.insert(&o.id) {
                return Err(ManifestError("duplicate resource override ID".into()));
            }
            dependencies = dependencies
                .checked_add(o.dependencies.len())
                .ok_or_else(|| ManifestError("dependency count overflow".into()))?;
            if dependencies > limits.manifest.max_dependencies {
                return Err(ManifestError("dependency count limit exceeded".into()));
            }
            if o.dependencies.iter().collect::<BTreeSet<_>>().len() != o.dependencies.len() {
                return Err(ManifestError("duplicate explicit dependency".into()));
            }
            for id in &o.dependencies {
                validate_id(id, limits.pack.max_id_bytes)?;
            }
            if o.variants.len() > 32 {
                return Err(ManifestError("variant count limit exceeded".into()));
            }
            for v in &o.variants {
                validate_id(v, 64)?;
            }
            if let Some(locale) = &o.locale {
                validate_id(locale, 64)?;
            }
        }
        Ok(())
    }
}
