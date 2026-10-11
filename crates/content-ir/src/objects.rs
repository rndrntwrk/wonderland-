// Source-derived resource binding follows FreeSO WorldObjectProvider.cs and
// VMThread.cs. Mozilla Public License, v. 2.0. https://mozilla.org/MPL/2.0/
//! Source resources and deterministic effective identities.

use crate::{
    patches::{apply_patches, iff_sha256, PatchFile, PatchProvenance},
    strings::LocaleSelection,
    tuning::{resolve_tuning, ResolvedTuning, TuningInputs},
};
use sha2::{Digest, Sha256};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk, IffFile},
    semantic::{decode_semantic, decode_ttab_with_variant, DecodedSemantic, TtabVariant},
    Error, ErrorKind, Limits, Result,
};

#[derive(Clone, Debug)]
pub struct SourceContent {
    pub name: String,
    pub iff: IffFile,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DependencyIdentity {
    pub name: String,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ResolveRequest {
    pub source: SourceContent,
    pub semiglobal: Option<SourceContent>,
    pub global: Option<SourceContent>,
    pub patches: Vec<PatchFile>,
    pub tuning: TuningInputs,
    pub locale: LocaleSelection,
    pub variant: String,
    pub rights: String,
    pub dependencies: Vec<DependencyIdentity>,
}

impl ResolveRequest {
    pub fn new(name: impl Into<String>, iff: IffFile) -> Self {
        Self {
            source: SourceContent {
                name: name.into(),
                iff,
            },
            semiglobal: None,
            global: None,
            patches: Vec::new(),
            tuning: TuningInputs::default(),
            locale: LocaleSelection::default(),
            variant: String::new(),
            rights: String::new(),
            dependencies: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourceIdentity {
    pub kind: [u8; 4],
    pub id: u16,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticEntry {
    pub key: ChunkKey,
    pub semantic: DecodedSemantic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResourceNamespace {
    Private,
    Semiglobal,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeProvenance {
    pub namespace: ResourceNamespace,
    pub source_name: String,
    pub source_hash: [u8; 32],
    pub effective_hash: [u8; 32],
    pub applied_patches: Vec<PatchProvenance>,
    pub suppressed_patches: Vec<PatchProvenance>,
}

#[derive(Clone, Debug)]
pub struct ResolvedContent {
    pub source_name: String,
    pub source_hash: [u8; 32],
    pub iff: IffFile,
    pub semantic_resources: Vec<SemanticEntry>,
    pub semiglobal_name: Option<String>,
    pub semiglobal: Option<SourceContent>,
    pub global: Option<SourceContent>,
    pub tuning: ResolvedTuning,
    pub identity: [u8; 32],
    /// Patches applied to the primary source, in explicit order.
    pub applied_patches: Vec<PatchProvenance>,
    pub suppressed_patches: Vec<PatchProvenance>,
    pub scope_provenance: Vec<ScopeProvenance>,
    pub resources: Vec<ResourceIdentity>,
}

#[derive(Clone, Copy)]
pub struct ResourceScope<'a> {
    pub private: &'a IffFile,
    pub semiglobal: Option<&'a IffFile>,
    pub global: Option<&'a IffFile>,
}

impl<'a> ResourceScope<'a> {
    pub fn lookup(
        &self,
        namespace: ResourceNamespace,
        kind: [u8; 4],
        id: u16,
    ) -> Option<&'a IffChunk> {
        let file = match namespace {
            ResourceNamespace::Private => Some(self.private),
            ResourceNamespace::Semiglobal => self.semiglobal,
            ResourceNamespace::Global => self.global,
        }?;
        file.chunks
            .iter()
            .find(|chunk| chunk.key == ChunkKey { kind, id })
    }
    /// VMThread resolves routine IDs by namespace; a miss does not search a
    /// different namespace and is distinct from primitive execution.
    pub fn lookup_bhav(&self, id: u16) -> Option<&'a IffChunk> {
        self.lookup(
            if id >= 8192 {
                ResourceNamespace::Semiglobal
            } else if id >= 4096 {
                ResourceNamespace::Private
            } else {
                ResourceNamespace::Global
            },
            *b"BHAV",
            id,
        )
    }
    /// Explicit tooling lookup order, for callers that require a search.
    pub fn lookup_ordered(
        &self,
        order: &[ResourceNamespace],
        kind: [u8; 4],
        id: u16,
    ) -> Option<&'a IffChunk> {
        order.iter().find_map(|scope| self.lookup(*scope, kind, id))
    }
}

impl ResolvedContent {
    pub fn scopes(&self) -> ResourceScope<'_> {
        ResourceScope {
            private: &self.iff,
            semiglobal: self.semiglobal.as_ref().map(|s| &s.iff),
            global: self.global.as_ref().map(|s| &s.iff),
        }
    }
}

fn hash_text(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u64).to_le_bytes());
    hash.update(text.as_bytes());
}

fn hash_patches(hash: &mut Sha256, patches: &[PatchProvenance]) {
    hash.update((patches.len() as u64).to_le_bytes());
    for patch in patches {
        hash_text(hash, &patch.name);
        hash_text(hash, &patch.source_name);
        hash.update([u8::from(patch.is_user)]);
        hash.update(patch.sha256);
    }
}

pub fn resource_identity(chunk: &IffChunk, limits: &Limits) -> Result<ResourceIdentity> {
    limits.check_count(
        chunk.data.len(),
        limits.max_resource_bytes,
        0,
        "resource identity bytes",
    )?;
    let mut hash = Sha256::new();
    hash.update(b"wonderland.resource.v1\0");
    hash.update(chunk.key.kind);
    hash.update(chunk.key.id.to_le_bytes());
    hash.update(chunk.flags.to_le_bytes());
    hash.update(chunk.label);
    hash.update((chunk.data.len() as u64).to_le_bytes());
    hash.update(&chunk.data);
    Ok(ResourceIdentity {
        kind: chunk.key.kind,
        id: chunk.key.id,
        sha256: hash.finalize().into(),
    })
}

pub fn sha256_hex(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in value {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn semantic_for(chunk: &IffChunk, variant: &str, limits: &Limits) -> Result<DecodedSemantic> {
    if chunk.key.kind == *b"TTAB" && variant == "tsbo" {
        Ok(DecodedSemantic::Ttab(decode_ttab_with_variant(
            &chunk.data,
            TtabVariant::Tsbo,
            limits,
        )?))
    } else {
        decode_semantic(chunk, limits)
    }
}

fn resolve_scope(
    source: &SourceContent,
    namespace: ResourceNamespace,
    request: &ResolveRequest,
    limits: &Limits,
) -> Result<(SourceContent, ScopeProvenance)> {
    let source_hash = iff_sha256(&source.iff, limits)?;
    let applied = apply_patches(&source.name, &source.iff, &request.patches, limits)?;
    for chunk in &applied.file.chunks {
        semantic_for(chunk, &request.variant, limits)?;
    }
    let effective_hash = iff_sha256(&applied.file, limits)?;
    Ok((
        SourceContent {
            name: source.name.clone(),
            iff: applied.file,
        },
        ScopeProvenance {
            namespace,
            source_name: source.name.clone(),
            source_hash,
            effective_hash,
            applied_patches: applied.applied,
            suppressed_patches: applied.suppressed,
        },
    ))
}

fn charge_retained(total: &mut usize, bytes: usize, limits: &Limits) -> Result<()> {
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content retained allocation"))?;
    limits.check_count(
        *total,
        limits.max_total_decoded_bytes,
        0,
        "content retained allocation",
    )
}

fn array_bytes<T>(count: usize) -> Result<usize> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content array bytes"))
}

fn file_heap(file: &IffFile) -> Result<usize> {
    file.chunks.iter().try_fold(
        array_bytes::<IffChunk>(file.chunks.capacity())?,
        |n, chunk| {
            n.checked_add(chunk.data.capacity())
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content file allocation"))
        },
    )
}

fn provenance_heap(patches: &Vec<PatchProvenance>) -> Result<usize> {
    patches.iter().try_fold(
        array_bytes::<PatchProvenance>(patches.capacity())?,
        |n, patch| {
            n.checked_add(patch.name.capacity())
                .and_then(|n| n.checked_add(patch.source_name.capacity()))
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content patch provenance"))
        },
    )
}

pub fn resolve_content(request: &ResolveRequest, limits: &Limits) -> Result<ResolvedContent> {
    for text in [&request.source.name, &request.variant, &request.rights] {
        limits.check_count(
            text.len(),
            limits.max_string_bytes,
            0,
            "content identity text",
        )?;
    }
    limits.check_count(
        request.dependencies.len(),
        limits.max_entries,
        0,
        "content dependencies",
    )?;
    let mut dependencies = std::collections::BTreeSet::new();
    for dependency in &request.dependencies {
        limits.check_count(
            dependency.name.len(),
            limits.max_string_bytes,
            0,
            "dependency identity name",
        )?;
        if !dependencies.insert(&dependency.name) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                0,
                "dependency identity name",
            ));
        }
    }
    let source_hash = iff_sha256(&request.source.iff, limits)?;
    let applied = apply_patches(
        &request.source.name,
        &request.source.iff,
        &request.patches,
        limits,
    )?;
    let mut scope_provenance = Vec::new();
    let semiglobal = if let Some(source) = &request.semiglobal {
        let (source, provenance) =
            resolve_scope(source, ResourceNamespace::Semiglobal, request, limits)?;
        scope_provenance.push(provenance);
        Some(source)
    } else {
        None
    };
    let global = if let Some(source) = &request.global {
        let (source, provenance) =
            resolve_scope(source, ResourceNamespace::Global, request, limits)?;
        scope_provenance.push(provenance);
        Some(source)
    } else {
        None
    };
    let mut total = applied.file.chunks.iter().try_fold(0usize, |n, chunk| {
        n.checked_add(chunk.data.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content total bytes"))
    })?;
    for scope in [&semiglobal, &global].into_iter().flatten() {
        for chunk in &scope.iff.chunks {
            total = total
                .checked_add(chunk.data.len())
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "content total bytes"))?;
        }
    }
    limits.check_count(
        total,
        limits.max_total_decoded_bytes,
        0,
        "content scope total bytes",
    )?;
    // Account for the retained result as a whole. Per-resource budgets alone
    // permit compact tables to expand repeatedly past the aggregate ceiling.
    let mut retained = 0usize;
    charge_retained(&mut retained, request.source.name.len(), limits)?;
    charge_retained(&mut retained, file_heap(&applied.file)?, limits)?;
    charge_retained(&mut retained, provenance_heap(&applied.applied)?, limits)?;
    charge_retained(&mut retained, provenance_heap(&applied.suppressed)?, limits)?;
    charge_retained(
        &mut retained,
        array_bytes::<ScopeProvenance>(scope_provenance.capacity())?,
        limits,
    )?;
    for scope in [&semiglobal, &global].into_iter().flatten() {
        charge_retained(&mut retained, file_heap(&scope.iff)?, limits)?;
        charge_retained(&mut retained, scope.name.capacity(), limits)?;
    }
    for scope in &scope_provenance {
        charge_retained(&mut retained, scope.source_name.capacity(), limits)?;
        charge_retained(
            &mut retained,
            provenance_heap(&scope.applied_patches)?,
            limits,
        )?;
        charge_retained(
            &mut retained,
            provenance_heap(&scope.suppressed_patches)?,
            limits,
        )?;
    }
    charge_retained(
        &mut retained,
        array_bytes::<SemanticEntry>(applied.file.chunks.len())?,
        limits,
    )?;
    charge_retained(
        &mut retained,
        array_bytes::<ResourceIdentity>(applied.file.chunks.len())?,
        limits,
    )?;
    let mut semantic_resources = Vec::with_capacity(applied.file.chunks.len());
    let mut resources = Vec::with_capacity(applied.file.chunks.len());
    for chunk in &applied.file.chunks {
        let decode_limits = Limits {
            max_total_decoded_bytes: limits.max_total_decoded_bytes - retained,
            ..*limits
        };
        let semantic = semantic_for(chunk, &request.variant, &decode_limits)?;
        charge_retained(&mut retained, semantic.retained_heap_bytes()?, limits)?;
        semantic_resources.push(SemanticEntry {
            key: chunk.key,
            semantic,
        });
        resources.push(resource_identity(chunk, limits)?);
    }
    let glob = semantic_resources.iter().find_map(|entry| {
        if let DecodedSemantic::Glob(glob) = &entry.semantic {
            Some(glob)
        } else {
            None
        }
    });
    let semiglobal_name = if let Some(glob) = glob.filter(|g| !g.name.bytes.is_empty()) {
        charge_retained(
            &mut retained,
            glob.name
                .bytes
                .len()
                .checked_mul(2)
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "decoded GLOB name"))?,
            limits,
        )?;
        Some(glob.name.text())
    } else {
        None
    };
    let tuning_limits = Limits {
        max_total_decoded_bytes: limits.max_total_decoded_bytes - retained,
        ..*limits
    };
    let tuning = resolve_tuning(
        &applied.file,
        semiglobal.as_ref().map(|s| &s.iff),
        global.as_ref().map(|s| &s.iff),
        &request.tuning,
        &tuning_limits,
    )?;
    charge_retained(&mut retained, tuning.retained_heap_bytes()?, limits)?;
    let mut identity = Sha256::new();
    identity.update(b"wonderland.effective-content.v1\0");
    hash_text(&mut identity, &request.source.name);
    identity.update(source_hash);
    identity.update(iff_sha256(&applied.file, limits)?);
    hash_patches(&mut identity, &applied.applied);
    identity.update((scope_provenance.len() as u64).to_le_bytes());
    for scope in &scope_provenance {
        identity.update([scope.namespace as u8]);
        hash_text(&mut identity, &scope.source_name);
        identity.update(scope.source_hash);
        identity.update(scope.effective_hash);
        hash_patches(&mut identity, &scope.applied_patches);
    }
    identity.update(tuning.identity);
    identity.update([request.locale.requested, request.locale.default_language]);
    hash_text(&mut identity, &request.variant);
    hash_text(&mut identity, &request.rights);
    identity.update((request.dependencies.len() as u64).to_le_bytes());
    for dependency in &request.dependencies {
        hash_text(&mut identity, &dependency.name);
        identity.update(dependency.sha256);
    }
    Ok(ResolvedContent {
        source_name: request.source.name.clone(),
        source_hash,
        iff: applied.file,
        semantic_resources,
        semiglobal_name,
        semiglobal,
        global,
        tuning,
        identity: identity.finalize().into(),
        applied_patches: applied.applied,
        suppressed_patches: applied.suppressed,
        scope_provenance,
        resources,
    })
}
