//! Verified cooked resources to the real runtime, without source-file lookup.
//!
//! A recipe defines an ordered cooked projection. It does not reconstruct an
//! original whole-IFF/resolver identity from derivative chunks or import reports.
//! Final bindings independently pin the manifest and expected runtime descriptor.
use crate::{
    budget::ImportBudget,
    content::{
        convert_content, ContentView, ConversionObject, ConvertedContent, ImportedInteraction,
        RuntimeMetadata, ScopeView,
    },
    cooked_json,
    cooked_metadata::{ImportOptionsV1, RuntimeMetadataV1, TtabVariantV1},
    SIM_CORE_REVISION,
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
};
use wonderland_content_ir::{
    manifest::{
        validate_id, AssetManifest, Digest, LoadPhase, LoadPlan, ManifestLimits, Provenance,
        ResourceCodec, ResourceKind,
    },
    packs::{verify_pack, PackEntry, PackIndex, PackLimits},
    tuning::ResolvedTuning,
    tuning_pack,
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic::{decode_semantic, decode_ttab_with_variant, DecodedSemantic},
    Limits,
};

pub const SOURCE_BASELINE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
pub const BINDING_VERSION: u32 = 1;
pub const MAX_REPORT_BYTES: usize = 1024 * 1024;

/// Count JSON output before allocating its exact buffer. Reports never truncate.
pub fn json_report<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    cooked_json::encode(value, MAX_REPORT_BYTES)
}

#[derive(Clone, Debug)]
pub struct CookedLoadLimits {
    pub max_binding_bytes: usize,
    pub max_total_pack_bytes: u64,
    pub manifest: ManifestLimits,
    pub pack: PackLimits,
    pub legacy: Limits,
}
impl Default for CookedLoadLimits {
    fn default() -> Self {
        Self {
            max_binding_bytes: 1024 * 1024,
            max_total_pack_bytes: 128 * 1024 * 1024,
            manifest: ManifestLimits::default(),
            pack: PackLimits::default(),
            legacy: Limits::default(),
        }
    }
}

fn required<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    T::deserialize(d)
}
fn object_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a JSON object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(ObjectVisitor(PhantomData))
}
macro_rules! record {
    ($name:ident { $( $field:ident: $ty:ty ),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty,)* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $(#[serde(deserialize_with="required")] $field: $ty,)* }
                let Fields { $($field,)* } = object_only(d)?;
                Ok(Self { $($field,)* })
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeNamespaceV1 {
    Private,
    Semiglobal,
    Global,
}
impl<'de> Deserialize<'de> for ScopeNamespaceV1 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "private" => Ok(Self::Private),
            "semiglobal" => Ok(Self::Semiglobal),
            "global" => Ok(Self::Global),
            _ => Err(de::Error::custom("invalid scope namespace")),
        }
    }
}
record!(ScopeBindingV1 { id: String, namespace: ScopeNamespaceV1, source_name: String, resources: Vec<String> });
record!(SemiglobalBindingV1 {
    scope: String,
    owner: u32
});
record!(ObjectBindingV1 {
    guid: u32, object_resource: String, private_scope: String,
    semiglobal: Option<SemiglobalBindingV1>, global_scope: Option<String>,
    tuning_resource: String, runtime: RuntimeMetadataV1,
});
record!(RuntimeRecipeV1 {
    sim_core_revision: String, source_baseline: String, manifest_sha256: Digest,
    variant: Option<String>, scopes: Vec<ScopeBindingV1>, objects: Vec<ObjectBindingV1>, options: ImportOptionsV1,
});
record!(RuntimeDescriptorV1 {
    content_sha256: Digest,
    tuning_sha256: Digest
});
impl From<sim_core::state::ContentDescriptor> for RuntimeDescriptorV1 {
    fn from(value: sim_core::state::ContentDescriptor) -> Self {
        Self {
            content_sha256: digest_words(value.content_hash),
            tuning_sha256: digest_words(value.tuning_hash),
        }
    }
}
record!(CookedRuntimeDraftV1 {
    schema_version: u32,
    recipe: RuntimeRecipeV1
});
record!(CookedRuntimeBindingV1 {
    schema_version: u32,
    recipe: RuntimeRecipeV1,
    expected_content: RuntimeDescriptorV1
});
impl CookedRuntimeBindingV1 {
    pub fn canonical_bytes(&self, limits: &CookedLoadLimits) -> Result<Vec<u8>, String> {
        if self.schema_version != BINDING_VERSION {
            return Err("unsupported cooked binding version".into());
        }
        cooked_json::encode(self, limits.max_binding_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackRequest {
    pub digest: Digest,
    pub byte_len: u64,
}
#[derive(Clone, Copy)]
pub struct PackBytes<'a> {
    pub digest: &'a Digest,
    pub bytes: &'a [u8],
}

struct PreparedCommon {
    recipe: RuntimeRecipeV1,
    manifest: AssetManifest,
    plan: LoadPlan,
    packs: Vec<PackRequest>,
    admission_bytes: usize,
}
pub struct PreparedDraft {
    common: PreparedCommon,
}
pub struct PreparedRelease {
    common: PreparedCommon,
    binding_sha256: Digest,
    expected: RuntimeDescriptorV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CookedObjectReport {
    pub guid: u32,
    pub object_resource: String,
    pub private_scope: String,
    pub semiglobal_owner: Option<u32>,
    pub interactions: Option<Vec<ImportedInteraction>>,
    pub global_interactions: Vec<ImportedInteraction>,
}
#[derive(Clone, Debug, Serialize)]
pub struct CookedReleaseReport {
    pub binding_sha256: Digest,
    pub manifest_sha256: Digest,
    pub runtime: RuntimeDescriptorV1,
    pub objects: Vec<CookedObjectReport>,
}
#[derive(Debug)]
pub struct LoadedRelease {
    pub content: sim_core::state::ContentSet,
    pub report: CookedReleaseReport,
}

impl PreparedDraft {
    /// Local preparation input. This API exposes no runtime from an unsealed recipe.
    pub fn from_json(
        bytes: &[u8],
        manifest: &[u8],
        limits: &CookedLoadLimits,
    ) -> Result<Self, String> {
        let mut budget = ImportBudget::new(&limits.legacy);
        let draft: CookedRuntimeDraftV1 =
            cooked_json::decode(bytes, limits.max_binding_bytes, &mut budget)?;
        if draft.schema_version != BINDING_VERSION {
            return Err("unsupported cooked draft version".into());
        }
        Ok(Self {
            common: PreparedCommon::new(draft.recipe, manifest, limits, budget)?,
        })
    }
    pub fn required_packs(&self) -> &[PackRequest] {
        &self.common.packs
    }
    pub fn seal(
        &self,
        packs: &[PackBytes<'_>],
        limits: &CookedLoadLimits,
    ) -> Result<CookedRuntimeBindingV1, String> {
        let converted = self.common.convert(packs, limits)?;
        Ok(CookedRuntimeBindingV1 {
            schema_version: BINDING_VERSION,
            recipe: self.common.recipe.clone(),
            expected_content: converted.content.descriptor()?.into(),
        })
    }
}
impl PreparedRelease {
    pub fn from_binding(
        bytes: &[u8],
        expected_sha256: &Digest,
        manifest: &[u8],
        limits: &CookedLoadLimits,
    ) -> Result<Self, String> {
        if bytes.len() > limits.max_binding_bytes || Digest::of(bytes) != *expected_sha256 {
            return Err("cooked binding size or trusted digest mismatch".into());
        }
        let mut budget = ImportBudget::new(&limits.legacy);
        let binding: CookedRuntimeBindingV1 =
            cooked_json::decode(bytes, limits.max_binding_bytes, &mut budget)?;
        if binding.schema_version != BINDING_VERSION {
            return Err("unsupported cooked binding version".into());
        }
        budget.reserve(64)?;
        Ok(Self {
            common: PreparedCommon::new(binding.recipe, manifest, limits, budget)?,
            binding_sha256: expected_sha256.clone(),
            expected: binding.expected_content,
        })
    }
    pub fn required_packs(&self) -> &[PackRequest] {
        &self.common.packs
    }
    pub fn plan(&self) -> &LoadPlan {
        &self.common.plan
    }
    pub fn plan_report(&self) -> Result<Vec<u8>, String> {
        #[derive(Serialize)]
        struct Report<'a> {
            binding_sha256: &'a Digest,
            manifest_sha256: &'a Digest,
            required_packs: &'a [PackRequest],
            plan: &'a LoadPlan,
        }
        json_report(&Report {
            binding_sha256: &self.binding_sha256,
            manifest_sha256: &self.common.recipe.manifest_sha256,
            required_packs: self.required_packs(),
            plan: self.plan(),
        })
    }
    pub fn load(
        &self,
        packs: &[PackBytes<'_>],
        limits: &CookedLoadLimits,
    ) -> Result<LoadedRelease, String> {
        let converted = self.common.convert(packs, limits)?;
        let descriptor = RuntimeDescriptorV1::from(converted.content.descriptor()?);
        if descriptor != self.expected {
            return Err("cooked runtime descriptor does not match sealed binding".into());
        }
        let objects = converted
            .objects
            .into_iter()
            .zip(&self.common.recipe.objects)
            .map(|(object, recipe)| CookedObjectReport {
                guid: object.guid,
                object_resource: recipe.object_resource.clone(),
                private_scope: recipe.private_scope.clone(),
                semiglobal_owner: object.semiglobal_owner,
                interactions: object.interactions,
                global_interactions: object.global_interactions,
            })
            .collect();
        Ok(LoadedRelease {
            content: converted.content,
            report: CookedReleaseReport {
                binding_sha256: self.binding_sha256.clone(),
                manifest_sha256: self.common.recipe.manifest_sha256.clone(),
                runtime: descriptor,
                objects,
            },
        })
    }
}

impl PreparedCommon {
    fn new(
        recipe: RuntimeRecipeV1,
        bytes: &[u8],
        limits: &CookedLoadLimits,
        mut budget: ImportBudget,
    ) -> Result<Self, String> {
        if bytes.len() > limits.manifest.max_manifest_bytes
            || Digest::of(bytes) != recipe.manifest_sha256
        {
            return Err("manifest size or trusted digest mismatch".into());
        }
        // The existing manifest decoder/validator enforces all version, graph,
        // ownership and fingerprint rules; preflight also admits its workspaces.
        let manifest: AssetManifest =
            cooked_json::decode(bytes, limits.manifest.max_manifest_bytes, &mut budget)?;
        manifest
            .validate(&limits.manifest)
            .map_err(|e| e.to_string())?;
        let roots = validate_recipe(&recipe, &manifest, limits, &mut budget)?;
        let plan = manifest
            .load_plan(
                &roots,
                LoadPhase::Simulation,
                recipe.variant.as_deref(),
                &BTreeSet::new(),
                &limits.manifest,
            )
            .map_err(|e| e.to_string())?;
        let selected: BTreeSet<_> = plan.resources.iter().collect();
        if selected != roots.iter().collect() {
            return Err(
                "critical dependency closure contains resources missing from the binding".into(),
            );
        }
        if plan.download_bytes > limits.max_total_pack_bytes {
            return Err("selected pack byte budget exceeded".into());
        }
        budget.entries::<PackRequest>(plan.packs.len())?;
        let packs = plan
            .packs
            .iter()
            .map(|digest| PackRequest {
                digest: digest.clone(),
                byte_len: manifest.packs[digest].byte_len,
            })
            .collect();
        let admission_bytes = budget.used();
        Ok(Self {
            recipe,
            manifest,
            plan,
            packs,
            admission_bytes,
        })
    }

    fn convert(
        &self,
        supplied: &[PackBytes<'_>],
        limits: &CookedLoadLimits,
    ) -> Result<ConvertedContent, String> {
        let mut budget = ImportBudget::new(&limits.legacy);
        budget.reserve(self.admission_bytes)?;
        if self.recipe.objects.len() > limits.legacy.max_entries
            || self.recipe.scopes.len() > limits.legacy.max_entries
        {
            return Err("cooked object or scope count limit".into());
        }
        let indexes =
            verify_selected_packs(supplied, &self.packs, &self.manifest, limits, &mut budget)?;
        budget.entries::<DecodedScope>(self.recipe.scopes.len())?;
        budget.map_entries::<&str, usize>(self.recipe.scopes.len())?;
        let mut scopes = Vec::with_capacity(self.recipe.scopes.len());
        let mut by_scope = BTreeMap::new();
        for binding in &self.recipe.scopes {
            let scope = decode_scope(
                binding,
                &self.manifest,
                &indexes,
                self.recipe.options.ttab_variant,
                limits,
                &mut budget,
            )?;
            by_scope.insert(binding.id.as_str(), scopes.len());
            scopes.push(scope);
        }
        let mut tunings = BTreeMap::new();
        budget.entries::<RuntimeMetadata>(self.recipe.objects.len())?;
        budget.entries::<ConversionObject<'_>>(self.recipe.objects.len())?;
        budget.entries::<CookedObjectReport>(self.recipe.objects.len())?;
        budget.reserve(256)?; // Digest strings in the final descriptor/report.
        let mut metadata = Vec::with_capacity(self.recipe.objects.len());
        for object in &self.recipe.objects {
            budget.reserve(object.object_resource.len() + object.private_scope.len())?;
            metadata.push(object.runtime.clone().into_runtime()?);
            if !tunings.contains_key(object.tuning_resource.as_str()) {
                budget.map_entries::<&str, ResolvedTuning>(1)?;
                let bytes = member_bytes(&object.tuning_resource, &self.manifest, &indexes)?;
                let tuning = tuning_pack::decode(bytes, &budget.limits(&limits.legacy))
                    .map_err(|e| e.to_string())?;
                budget.reserve(tuning.retained_heap_bytes().map_err(|e| e.to_string())?)?;
                let identity = digest_words(tuning.identity);
                let record = &self.manifest.resources[&object.tuning_resource];
                if record.provenance.tuning_hash.as_ref() != Some(&identity) {
                    return Err("cooked tuning identity differs from manifest provenance".into());
                }
                tunings.insert(object.tuning_resource.as_str(), tuning);
            }
        }
        let mut inputs = Vec::with_capacity(self.recipe.objects.len());
        for (object, runtime) in self.recipe.objects.iter().zip(&metadata) {
            let private_index = by_scope[object.private_scope.as_str()];
            let private = &scopes[private_index];
            let private_binding = &self.recipe.scopes[private_index];
            let object_index = private_binding
                .resources
                .iter()
                .position(|id| id == &object.object_resource)
                .ok_or("OBJD member is absent from private scope")?;
            let selected = &private.iff.chunks[object_index];
            if selected.key.kind != *b"OBJD" {
                return Err("object binding member must contain OBJD".into());
            }
            let raw =
                crate::content::decode_source_objd(&selected.data, &budget.limits(&limits.legacy))?;
            budget.entries::<u16>(raw.fields.capacity())?;
            if raw.guid() != object.guid {
                return Err("object GUID differs from bound OBJD member".into());
            }
            let tuning = &tunings[object.tuning_resource.as_str()];
            if tuning.has_semiglobal != object.semiglobal.is_some()
                || (object.global_scope.is_none() && !tuning.global_cache.is_empty())
            {
                return Err("cooked tuning scope presence differs from object binding".into());
            }
            let tuning_identity = self.manifest.resources[&object.tuning_resource]
                .provenance
                .tuning_hash
                .as_ref();
            let view = |id: &str| -> Result<ScopeView<'_>, String> {
                let index = by_scope[id];
                if scopes[index].provenance.tuning_hash.as_ref() != tuning_identity {
                    return Err(
                        "scope and object tuning belong to different provenance cohorts".into(),
                    );
                }
                Ok(ScopeView {
                    name: &self.recipe.scopes[index].source_name,
                    iff: &scopes[index].iff,
                    identity: scopes[index].identity,
                })
            };
            let semiglobal = object
                .semiglobal
                .as_ref()
                .map(|s| view(&s.scope))
                .transpose()?;
            let global = object.global_scope.as_ref().map(|s| view(s)).transpose()?;
            inputs.push(ConversionObject {
                source: ContentView {
                    iff: &private.iff,
                    semiglobal_name: private.semiglobal_name.as_deref(),
                    semiglobal,
                    global,
                    tuning,
                    resolved: None,
                },
                object_chunk_id: selected.key.id,
                semiglobal_owner: object.semiglobal.as_ref().map(|s| s.owner),
                runtime,
            });
        }
        convert_content(
            &inputs,
            self.recipe.options.clone().into_runtime()?,
            &limits.legacy,
            &mut budget,
        )
    }
}

fn validate_recipe(
    recipe: &RuntimeRecipeV1,
    manifest: &AssetManifest,
    limits: &CookedLoadLimits,
    budget: &mut ImportBudget,
) -> Result<Vec<String>, String> {
    if recipe.sim_core_revision != SIM_CORE_REVISION
        || recipe.source_baseline != SOURCE_BASELINE
        || manifest.source_baseline != SOURCE_BASELINE
    {
        return Err("cooked binding source or runtime revision mismatch".into());
    }
    if recipe.objects.is_empty()
        || recipe.objects.len() > 32767
        || recipe.objects.len() > limits.legacy.max_entries
        || recipe.scopes.is_empty()
        || recipe.scopes.len() > limits.legacy.max_entries
    {
        return Err("cooked object or scope count limit".into());
    }
    let mut scopes = BTreeMap::new();
    let mut members = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for scope in &recipe.scopes {
        validate_id(&scope.id, limits.manifest.max_string_bytes).map_err(|e| e.to_string())?;
        if scope.source_name.is_empty()
            || scope.source_name.len() > limits.manifest.max_string_bytes
            || scope.source_name.chars().any(char::is_control)
            || scope.resources.is_empty()
            || scope.resources.len() > limits.legacy.max_entries
        {
            return Err("invalid cooked scope name or member count".into());
        }
        budget.map_entries::<&str, &ScopeBindingV1>(1)?;
        if scopes.insert(scope.id.as_str(), scope).is_some() {
            return Err("duplicate cooked scope identity".into());
        }
        let mut cohort = None;
        for id in &scope.resources {
            budget.map_entries::<&str, ()>(2)?;
            let record = manifest
                .resources
                .get(id)
                .ok_or("bound scope member is absent from manifest")?;
            if !members.insert(id.as_str()) {
                return Err("duplicate cooked member or cross-scope alias".into());
            }
            if record.kind != ResourceKind::Semantic
                || !record.simulation_critical
                || !matches!(
                    record.codec,
                    ResourceCodec::IffChunk | ResourceCodec::IffTtabTsbo
                )
            {
                return Err("scope member must be a critical semantic IFF resource".into());
            }
            if record.provenance.tuning_hash.is_none() {
                return Err("scope member lacks tuning provenance".into());
            }
            if cohort
                .replace(&record.provenance)
                .is_some_and(|previous| previous != &record.provenance)
            {
                return Err("mixed provenance cohort in cooked scope".into());
            }
            roots.insert(id.as_str());
        }
    }
    let mut used = BTreeSet::new();
    let mut guids = BTreeSet::new();
    for object in &recipe.objects {
        budget.map_entries::<u32, ()>(1)?;
        if object.guid == 0 || !guids.insert(object.guid) {
            return Err("zero or duplicate cooked object GUID".into());
        }
        let mut take_scope = |id: &str, namespace| -> Result<&ScopeBindingV1, String> {
            let scope = *scopes
                .get(id)
                .ok_or("object refers to missing cooked scope")?;
            if scope.namespace != namespace {
                return Err("object scope namespace mismatch".into());
            }
            used.insert(scope.id.as_str());
            Ok(scope)
        };
        let private = take_scope(&object.private_scope, ScopeNamespaceV1::Private)?;
        if !private.resources.contains(&object.object_resource) {
            return Err("object member does not belong to its private scope".into());
        }
        if let Some(semi) = &object.semiglobal {
            if semi.owner == 0 {
                return Err("semiglobal owner must be nonzero".into());
            }
            take_scope(&semi.scope, ScopeNamespaceV1::Semiglobal)?;
        }
        if let Some(global) = &object.global_scope {
            take_scope(global, ScopeNamespaceV1::Global)?;
        }
        let record = manifest
            .resources
            .get(&object.tuning_resource)
            .ok_or("bound tuning member is absent from manifest")?;
        if record.kind != ResourceKind::Semantic
            || record.codec != ResourceCodec::ResolvedTuning
            || !record.simulation_critical
        {
            return Err("object tuning must be a critical resolved-tuning resource".into());
        }
        if record.provenance != manifest.resources[&private.resources[0]].provenance {
            return Err("private scope and tuning belong to different provenance cohorts".into());
        }
        budget.map_entries::<&str, ()>(1)?;
        roots.insert(object.tuning_resource.as_str());
    }
    if used.len() != scopes.len() {
        return Err("cooked binding contains unused scopes".into());
    }
    if roots.len() > limits.manifest.max_resources {
        return Err("bound resource count limit".into());
    }
    budget.entries::<String>(roots.len())?;
    for id in &roots {
        budget.reserve(id.len())?;
    }
    Ok(roots.into_iter().map(str::to_owned).collect())
}

fn verify_selected_packs<'a>(
    supplied: &[PackBytes<'a>],
    requests: &[PackRequest],
    manifest: &AssetManifest,
    limits: &CookedLoadLimits,
    budget: &mut ImportBudget,
) -> Result<BTreeMap<Digest, PackIndex<'a>>, String> {
    if supplied.len() != requests.len() || supplied.len() > limits.manifest.max_packs {
        return Err("supplied packs do not exactly cover selected pack identities".into());
    }
    let mut total = 0u64;
    let mut indexes = BTreeMap::new();
    for pack in supplied {
        let request = requests
            .iter()
            .find(|r| &r.digest == pack.digest)
            .ok_or("unexpected supplied pack identity")?;
        if pack.bytes.len() as u64 != request.byte_len
            || pack.bytes.len() > limits.pack.max_pack_bytes
        {
            return Err("supplied pack length differs from manifest or limit".into());
        }
        total = total
            .checked_add(request.byte_len)
            .filter(|n| *n <= limits.max_total_pack_bytes)
            .ok_or("selected pack byte budget exceeded")?;
        if indexes.contains_key(pack.digest) {
            return Err("duplicate supplied pack identity".into());
        }
        let count = u32::from_le_bytes(
            pack.bytes
                .get(12..16)
                .ok_or("truncated pack header")?
                .try_into()
                .unwrap(),
        ) as usize;
        if count > limits.pack.max_resources {
            return Err("pack resource count limit".into());
        }
        let index_len = u64::from_le_bytes(
            pack.bytes
                .get(16..24)
                .ok_or("truncated pack header")?
                .try_into()
                .unwrap(),
        );
        let index_len = usize::try_from(index_len).map_err(|_| "pack index size overflow")?;
        if index_len > pack.bytes.len() {
            return Err("pack index lies outside input".into());
        }
        budget.map_entries::<String, PackEntry>(count)?;
        budget.reserve(
            index_len
                .checked_mul(3)
                .ok_or("pack index workspace overflow")?,
        )?;
        budget.map_entries::<Digest, PackIndex<'_>>(1)?;
        budget.reserve(128)?;
        let index =
            verify_pack(pack.bytes, pack.digest, &limits.pack).map_err(|e| e.to_string())?;
        index
            .validate_manifest(manifest, &limits.manifest)
            .map_err(|e| e.to_string())?;
        indexes.insert(pack.digest.clone(), index);
    }
    Ok(indexes)
}

fn member_bytes<'a>(
    id: &str,
    manifest: &AssetManifest,
    indexes: &BTreeMap<Digest, PackIndex<'a>>,
) -> Result<&'a [u8], String> {
    let record = manifest.resources.get(id).ok_or("missing bound resource")?;
    indexes
        .get(&record.pack)
        .ok_or("missing verified selected pack")?
        .get(id)
        .map_err(|e| e.to_string())
}
struct DecodedScope {
    iff: IffFile,
    identity: [u8; 32],
    semiglobal_name: Option<String>,
    provenance: Provenance,
}
fn decode_scope(
    binding: &ScopeBindingV1,
    manifest: &AssetManifest,
    indexes: &BTreeMap<Digest, PackIndex<'_>>,
    variant: TtabVariantV1,
    limits: &CookedLoadLimits,
    budget: &mut ImportBudget,
) -> Result<DecodedScope, String> {
    budget.entries::<IffChunk>(binding.resources.len())?;
    let mut chunks = Vec::with_capacity(binding.resources.len());
    let mut header = None;
    let mut keys = BTreeSet::new();
    let mut hash = Sha256::new();
    let mut semiglobal_name = None;
    let mut seen_glob = false;
    for id in &binding.resources {
        let record = &manifest.resources[id];
        let bytes = member_bytes(id, manifest, indexes)?;
        if bytes.len() < 140
            || bytes[60..64] != [0; 4]
            || u32::from_be_bytes(bytes[68..72].try_into().unwrap()) as usize != bytes.len() - 64
        {
            return Err("cooked IFF member must contain exactly one unindexed chunk".into());
        }
        budget.entries::<IffFile>(1)?;
        budget.entries::<IffChunk>(4)?; // The legacy decoder starts with a small Vec.
        budget.map_entries::<ChunkKey, ()>(2)?;
        budget.reserve(bytes.len())?;
        let admitted = Limits {
            max_entries: 1,
            max_total_decoded_bytes: bytes.len(),
            ..budget.limits(&limits.legacy)
        };
        let mut member = iff::decode(bytes, &admitted).map_err(|e| e.to_string())?;
        let chunk = member.chunks.pop().ok_or("empty cooked IFF member")?;
        if !keys.insert(chunk.key) {
            return Err("duplicate chunk key in cooked scope".into());
        }
        match header {
            Some(expected) if member.header != expected => {
                return Err("mixed IFF headers in cooked scope".into())
            }
            None => {
                header = Some(member.header);
                hash.update(member.header);
            }
            _ => {}
        }
        hash.update(&bytes[64..]);
        if chunk.key.kind == *b"PIFF" {
            return Err("unapplied PIFF is not a runtime scope member".into());
        }
        let decoded = if chunk.key.kind == *b"TTAB" {
            let expected = if variant == TtabVariantV1::Tsbo {
                ResourceCodec::IffTtabTsbo
            } else {
                ResourceCodec::IffChunk
            };
            if record.codec != expected {
                return Err("TTAB codec differs from bound runtime dialect".into());
            }
            let count = u16::from_le_bytes(
                chunk
                    .data
                    .get(..2)
                    .ok_or("truncated TTAB count")?
                    .try_into()
                    .unwrap(),
            ) as usize;
            budget.map_entries::<u32, ()>(count)?;
            DecodedSemantic::Ttab(
                decode_ttab_with_variant(
                    &chunk.data,
                    variant.into(),
                    &budget.limits(&limits.legacy),
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            if record.codec != ResourceCodec::IffChunk {
                return Err("non-TTAB member has TTAB-only codec".into());
            }
            decode_semantic(&chunk, &budget.limits(&limits.legacy)).map_err(|e| e.to_string())?
        };
        budget.reserve(decoded.retained_heap_bytes().map_err(|e| e.to_string())?)?;
        if matches!(decoded, DecodedSemantic::Unknown { .. }) {
            return Err("unknown payload is not a semantic runtime member".into());
        }
        if let DecodedSemantic::Glob(glob) = &decoded {
            if !seen_glob {
                seen_glob = true;
                if !glob.name.bytes.is_empty() {
                    semiglobal_name = Some(budget.decode_text(&glob.name)?);
                }
            }
        }
        chunks.push(chunk);
    }
    Ok(DecodedScope {
        iff: IffFile {
            header: header.ok_or("empty cooked scope")?,
            chunks,
        },
        identity: hash.finalize().into(),
        semiglobal_name,
        provenance: manifest.resources[&binding.resources[0]].provenance.clone(),
    })
}

fn digest_words(words: [u8; 32]) -> Digest {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in words {
        text.push(HEX[usize::from(byte >> 4)] as char);
        text.push(HEX[usize::from(byte & 15)] as char);
    }
    Digest::try_from(text).expect("32-byte digest encoded as lowercase hex")
}
