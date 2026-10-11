// SPDX-License-Identifier: MPL-2.0
//! Real source imports through bounded archive readers and the ordered content resolver.
pub mod obj;
use crate::{
    fs_io::read_import,
    manifest::{CookLimits, PreparedResource},
    registry::{CookSpec, SourceFormat, SourceSpec},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use wonderland_content_ir::{
    manifest::*,
    objects::{resolve_content, sha256_hex, ResolveRequest},
    patches::PatchFile,
};
use wonderland_legacy_formats::{
    self as legacy,
    iff::{self, IffChunk, IffFile},
    semantic::{self, DecodedSemantic},
    sprites,
};
#[derive(Debug, Serialize)]
pub struct ResourceMapping {
    pub id: String,
    pub source_id: String,
    pub filename: String,
    pub archive_key: Option<legacy::ResourceKey>,
    pub chunk_kind_hex: Option<String>,
    pub chunk_id: Option<u16>,
    pub classification: ResourceKind,
}
#[derive(Debug, Serialize)]
pub struct SourceReport {
    pub id: String,
    pub source_name: String,
    pub original_sha256: Digest,
    pub resolver_identity: Option<String>,
    pub applied_patches: Vec<wonderland_content_ir::patches::PatchProvenance>,
    pub suppressed_patches: Vec<wonderland_content_ir::patches::PatchProvenance>,
}
#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub schema_version: u32,
    pub fixture_only: bool,
    pub sources: Vec<SourceReport>,
    pub resources: Vec<ResourceMapping>,
    pub limitations: Vec<String>,
    pub input_hashes: BTreeMap<String, Digest>,
}
impl ImportReport {
    pub fn canonical_bytes(&self, limit: usize) -> ManifestResult<Vec<u8>> {
        struct Bounded {
            bytes: Vec<u8>,
            limit: usize,
        }
        impl std::io::Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self
                    .bytes
                    .len()
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > self.limit)
                {
                    return Err(std::io::Error::other("import report byte limit exceeded"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Bounded {
            bytes: Vec::new(),
            limit,
        };
        serde_json::to_writer(&mut writer, self).map_err(err)?;
        Ok(writer.bytes)
    }
}
#[derive(Debug)]
pub struct ImportedContent {
    pub resources: Vec<PreparedResource>,
    pub report: ImportReport,
}
fn err(e: impl std::fmt::Display) -> ManifestError {
    ManifestError(e.to_string())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn chunk_id(prefix: &str, chunk: &IffChunk) -> String {
    format!(
        "{prefix}/chunk-{}-{:04x}",
        hex(&chunk.key.kind),
        chunk.key.id
    )
}
pub fn archive_id(prefix: &str, key: &legacy::ResourceKey) -> String {
    match key {
        legacy::ResourceKey::Far1 { name } => format!("{prefix}/far1-{}", hex(name)),
        legacy::ResourceKey::Far3 { type_id, file_id } => {
            format!("{prefix}/far3-{type_id:08x}-{file_id:08x}")
        }
        legacy::ResourceKey::Dbpf {
            type_id,
            group_id,
            instance_id,
        } => format!("{prefix}/dbpf-{type_id:08x}-{group_id:08x}-{instance_id:08x}"),
    }
}
fn provenance(
    source: &SourceSpec,
    hash: &Digest,
    tuning: &Digest,
    patches: Vec<Digest>,
) -> Provenance {
    Provenance {
        origin: source.provenance.origin,
        source: source.path.clone(),
        source_hash: hash.clone(),
        patch_hashes: patches,
        tuning_hash: Some(tuning.clone()),
        license: source.provenance.license.clone(),
        redistribution: source.provenance.redistribution,
    }
}
struct ProvenanceInputs<'a> {
    source_hash: &'a Digest,
    tuning_hash: &'a Digest,
    applied: &'a [Digest],
}
struct Importer<'a> {
    spec: &'a CookSpec,
    root: &'a Path,
    limits: &'a CookLimits,
    output: ImportedContent,
    read_total: usize,
    decoded_total: usize,
}
impl Importer<'_> {
    fn read(&mut self, path: &str) -> ManifestResult<Vec<u8>> {
        let remaining = self
            .limits
            .legacy
            .max_total_decoded_bytes
            .saturating_sub(self.read_total);
        let bytes = read_import(
            self.root,
            path,
            remaining.min(self.limits.legacy.max_input_bytes),
        )?;
        self.read_total = self
            .read_total
            .checked_add(bytes.len())
            .ok_or_else(|| err("aggregate source bytes overflow"))?;
        self.output
            .report
            .input_hashes
            .insert(path.into(), Digest::of(&bytes));
        Ok(bytes)
    }
    fn add(&mut self, resource: PreparedResource, mapping: ResourceMapping) -> ManifestResult<()> {
        validate_id(&resource.id, self.limits.pack.max_id_bytes)?;
        if self.output.resources.len() >= self.limits.manifest.max_resources {
            return Err(err("import resource count exceeded"));
        }
        self.decoded_total = self
            .decoded_total
            .checked_add(resource.payload.len())
            .ok_or_else(|| err("decoded total overflow"))?;
        if self.decoded_total > self.limits.legacy.max_total_decoded_bytes {
            return Err(err("aggregate decoded bytes exceeded"));
        }
        self.output.resources.push(resource);
        self.output.report.resources.push(mapping);
        Ok(())
    }
    fn iff(
        &mut self,
        source: &SourceSpec,
        prefix: &str,
        bytes: &[u8],
        name: String,
        key: Option<legacy::ResourceKey>,
        source_hash: &Digest,
    ) -> ManifestResult<()> {
        let file = iff::decode(bytes, &self.limits.legacy).map_err(err)?;
        let mut request = ResolveRequest::new(name, file);
        request.rights = serde_json::to_string(&source.provenance).map_err(err)?;
        request.variant = source.resolver_variant.clone();
        request.locale = wonderland_content_ir::strings::LocaleSelection {
            requested: source.locale_selection.requested,
            default_language: source.locale_selection.default_language,
        };
        let mut scope_hashes = BTreeMap::new();
        for (namespace, scope) in [
            ("semiglobal", source.semiglobal.as_ref()),
            ("global", source.global.as_ref()),
        ] {
            if let Some(scope) = scope {
                let bytes = self.read(&scope.path)?;
                scope_hashes.insert(namespace, Digest::of(&bytes));
                let content = wonderland_content_ir::objects::SourceContent {
                    name: scope.source_name.clone(),
                    iff: iff::decode(&bytes, &self.limits.legacy).map_err(err)?,
                };
                if namespace == "semiglobal" {
                    request.semiglobal = Some(content);
                } else {
                    request.global = Some(content);
                }
            }
        }
        for (rewrite, path) in [
            (false, source.tuning.otf.as_ref()),
            (true, source.tuning.otf_rewrite.as_ref()),
        ] {
            if let Some(path) = path {
                let bytes = self.read(path)?;
                let otf = semantic::decode_otf(&bytes, &self.limits.legacy).map_err(err)?;
                if rewrite {
                    request.tuning.otf_rewrite = Some(otf);
                } else {
                    request.tuning.otf = Some(otf);
                }
            }
        }
        request.tuning.upgrades = source
            .tuning
            .upgrades
            .iter()
            .map(|v| wonderland_content_ir::tuning::TuningOverride {
                table: v.table,
                index: v.index,
                value: v.value,
            })
            .collect();
        request.tuning.dynamic_private = source
            .tuning
            .dynamic_private
            .iter()
            .map(|v| wonderland_content_ir::tuning::DynamicOverride {
                table: v.table,
                index: v.index,
                value_bits: v.value_bits,
            })
            .collect();
        request.tuning.dynamic_semiglobal = source
            .tuning
            .dynamic_semiglobal
            .iter()
            .map(|v| wonderland_content_ir::tuning::DynamicOverride {
                table: v.table,
                index: v.index,
                value_bits: v.value_bits,
            })
            .collect();
        let mut patch_hashes = BTreeMap::new();
        for patch in &source.patches {
            let bytes = self.read(&patch.path)?;
            patch_hashes.insert(request.patches.len(), Digest::of(&bytes));
            request.patches.push(PatchFile {
                name: patch.path.clone(),
                is_user: patch.is_user,
                file: iff::decode(&bytes, &self.limits.legacy).map_err(err)?,
            });
        }
        let resolved = resolve_content(&request, &self.limits.legacy).map_err(err)?;
        let tuning_hash = Digest::try_from(sha256_hex(&resolved.tuning.identity))?;
        let applied: Vec<Digest> = resolved
            .applied_patches
            .iter()
            .map(|p| patch_hashes[&p.order].clone())
            .collect();
        self.emit_chunks(
            source,
            prefix,
            &resolved.iff,
            key.clone(),
            ProvenanceInputs {
                source_hash,
                tuning_hash: &tuning_hash,
                applied: &applied,
            },
        )?;
        for (namespace, content, scope) in [
            (
                "semiglobal",
                resolved.semiglobal.as_ref(),
                source.semiglobal.as_ref(),
            ),
            ("global", resolved.global.as_ref(), source.global.as_ref()),
        ] {
            if let (Some(content), Some(scope)) = (content, scope) {
                let mut scope_source = source.clone();
                scope_source.path = scope.path.clone();
                let provenance = resolved
                    .scope_provenance
                    .iter()
                    .find(|p| p.source_name == scope.source_name && ((namespace=="semiglobal" && p.namespace==wonderland_content_ir::objects::ResourceNamespace::Semiglobal) || (namespace=="global" && p.namespace==wonderland_content_ir::objects::ResourceNamespace::Global)))
                    .ok_or_else(|| err("resolved scope provenance missing"))?;
                let applied: Vec<Digest> = provenance
                    .applied_patches
                    .iter()
                    .map(|p| patch_hashes[&p.order].clone())
                    .collect();
                let scope_prefix = format!("{prefix}/{namespace}");
                self.emit_chunks(
                    &scope_source,
                    &scope_prefix,
                    &content.iff,
                    None,
                    ProvenanceInputs {
                        source_hash: &scope_hashes[namespace],
                        tuning_hash: &tuning_hash,
                        applied: &applied,
                    },
                )?;
                self.output.report.sources.push(SourceReport {
                    id: scope_prefix,
                    source_name: scope.source_name.clone(),
                    original_sha256: scope_hashes[namespace].clone(),
                    resolver_identity: Some(sha256_hex(&provenance.effective_hash)),
                    applied_patches: provenance.applied_patches.clone(),
                    suppressed_patches: provenance.suppressed_patches.clone(),
                });
            }
        }
        let id = format!("{prefix}/resolved-tuning");
        let payload =
            wonderland_content_ir::tuning_pack::encode(&resolved.tuning, &self.limits.legacy)
                .map_err(err)?;
        self.add(
            PreparedResource {
                id: id.clone(),
                group: source.pack_group.clone(),
                kind: ResourceKind::Semantic,
                codec: ResourceCodec::ResolvedTuning,
                payload,
                dependencies: Vec::new(),
                simulation_critical: false,
                locale: None,
                variants: BTreeSet::new(),
                provenance: provenance(source, source_hash, &tuning_hash, applied),
            },
            ResourceMapping {
                id,
                source_id: source.id.clone(),
                filename: source.path.clone(),
                archive_key: key,
                chunk_kind_hex: None,
                chunk_id: None,
                classification: ResourceKind::Semantic,
            },
        )?;
        self.output.report.sources.push(SourceReport {
            id: prefix.into(),
            source_name: resolved.source_name,
            original_sha256: Digest::of(bytes),
            resolver_identity: Some(sha256_hex(&resolved.identity)),
            applied_patches: resolved.applied_patches,
            suppressed_patches: resolved.suppressed_patches,
        });
        Ok(())
    }
    fn emit_chunks(
        &mut self,
        source: &SourceSpec,
        prefix: &str,
        file: &IffFile,
        key: Option<legacy::ResourceKey>,
        inputs: ProvenanceInputs<'_>,
    ) -> ManifestResult<()> {
        let mut palettes = BTreeMap::new();
        for chunk in &file.chunks {
            if chunk.key.kind == *b"PALT" {
                palettes.insert(
                    chunk.key.id,
                    sprites::decode_palt(&chunk.data, &self.limits.legacy).map_err(err)?,
                );
            }
        }
        let mut sprite_chunks: BTreeMap<u32, Vec<&IffChunk>> = BTreeMap::new();
        for chunk in &file.chunks {
            if matches!(&chunk.key.kind, b"SPR#" | b"SPR2") {
                sprite_chunks
                    .entry(u32::from(chunk.key.id))
                    .or_default()
                    .push(chunk);
            }
        }
        for chunk in &file.chunks {
            let semantic = if chunk.key.kind == *b"TTAB" && source.resolver_variant == "tsbo" {
                DecodedSemantic::Ttab(
                    semantic::decode_ttab_with_variant(
                        &chunk.data,
                        semantic::TtabVariant::Tsbo,
                        &self.limits.legacy,
                    )
                    .map_err(err)?,
                )
            } else {
                semantic::decode_semantic(chunk, &self.limits.legacy).map_err(err)?
            };
            let mut kind = if matches!(
                semantic,
                DecodedSemantic::Unknown { .. } | DecodedSemantic::Piff(_)
            ) {
                ResourceKind::Opaque
            } else {
                ResourceKind::Semantic
            };
            let mut dependencies = BTreeSet::new();
            match &chunk.key.kind {
                b"PALT" => kind = ResourceKind::Visual,
                b"DGRP" => {
                    let group =
                        sprites::decode_dgrp(&chunk.data, &self.limits.legacy).map_err(err)?;
                    for sprite in group.images.iter().flat_map(|image| &image.sprites) {
                        let candidates = sprite_chunks
                            .get(&sprite.sprite_id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        if candidates.len() != 1 {
                            return Err(err(format!(
                                "DGRP sprite {} requires one unambiguous source SPR#/SPR2 resource",
                                sprite.sprite_id
                            )));
                        }
                        dependencies.insert(chunk_id(prefix, candidates[0]));
                    }
                    kind = ResourceKind::Visual;
                }
                b"SPR2" => {
                    let set = sprites::decode_spr2_with_palettes(
                        &chunk.data,
                        |id| palettes.get(&id),
                        &self.limits.legacy,
                    )
                    .map_err(err)?;
                    let mut ids: BTreeSet<u16> =
                        set.frames.iter().map(|frame| frame.palette_id).collect();
                    if let Ok(default) = u16::try_from(set.default_palette_id) {
                        if palettes.contains_key(&default) {
                            ids.insert(default);
                        }
                    }
                    for id in ids {
                        if !palettes.contains_key(&id) {
                            return Err(err("SPR2 palette resource unavailable"));
                        }
                        dependencies.insert(format!("{prefix}/chunk-50414c54-{id:04x}"));
                    }
                    kind = ResourceKind::Visual;
                }
                b"SPR#" => {
                    if chunk.data.len() < 12 {
                        return Err(err("SPR# header is truncated"));
                    }
                    let little = u32::from_le_bytes(chunk.data[0..4].try_into().unwrap());
                    let palette = if matches!(little, 1000 | 1001) {
                        u32::from_le_bytes(chunk.data[8..12].try_into().unwrap())
                    } else {
                        u32::from_be_bytes(chunk.data[8..12].try_into().unwrap())
                    };
                    let palette_id = u16::try_from(palette)
                        .map_err(|_| err("SPR# palette ID exceeds source resource identity"))?;
                    dependencies.insert(format!("{prefix}/chunk-50414c54-{palette_id:04x}"));
                    let palette = u16::try_from(palette)
                        .ok()
                        .and_then(|id| palettes.get(&id))
                        .ok_or_else(|| err("SPR# source PALT dependency unavailable"))?;
                    sprites::decode_spr(&chunk.data, palette, &self.limits.legacy).map_err(err)?;
                    kind = ResourceKind::Visual;
                }
                b"FSOM" => {
                    let mesh =
                        legacy::reconstruction::decode_fsom(&chunk.data, &self.limits.legacy)
                            .map_err(err)?;
                    let drawing = file
                        .chunks
                        .iter()
                        .find(|c| c.key.kind == *b"DGRP" && c.key.id == chunk.key.id)
                        .ok_or_else(|| err("FSOM requires its same-ID source DGRP"))?;
                    let group =
                        sprites::decode_dgrp(&drawing.data, &self.limits.legacy).map_err(err)?;
                    dependencies.insert(chunk_id(prefix, drawing));
                    for geometry in mesh.groups.iter().flatten().chain(mesh.depth_mask.iter()) {
                        match geometry.texture_reference() {
                            legacy::reconstruction::FsomTextureReference::CustomTexture { id } => {
                                let texture=file.chunks.iter().find(|c|c.key.kind==*b"MTEX"&&c.key.id==id)
                                    .ok_or_else(||err(format!("FSOM custom texture {id} requires an explicit same-IFF MTEX; external replacement textures are not discovered")))?;
                                dependencies.insert(chunk_id(prefix, texture));
                            }
                            legacy::reconstruction::FsomTextureReference::Sprite {
                                index,
                                rotation,
                            } => {
                                let sprite=group.image(1,3,u32::from(rotation)).and_then(|image|image.sprites.get(index as usize))
                                    .ok_or_else(||err(format!("FSOM sprite rotation {rotation}, index {index} has no exact source DGRP image")))?;
                                let candidates = sprite_chunks
                                    .get(&sprite.sprite_id)
                                    .map(Vec::as_slice)
                                    .unwrap_or(&[]);
                                if candidates.len() != 1 {
                                    return Err(err(
                                        "FSOM texture needs one unambiguous SPR#/SPR2 resource",
                                    ));
                                }
                                dependencies.insert(chunk_id(prefix, candidates[0]));
                            }
                        }
                    }
                    kind = ResourceKind::Visual;
                }
                b"MTEX" if chunk.data.starts_with(b"\x89PNG\r\n\x1a\n") => {
                    legacy::textures::decode_png(&chunk.data, &self.limits.legacy).map_err(err)?;
                    kind = ResourceKind::Visual;
                }
                _ => {}
            }
            let id = chunk_id(prefix, chunk);
            // A declared one-chunk derivative keeps all chunk fields and source header bytes,
            // except the original whole-file rsmp offset, which has no meaning in this member.
            let mut header = file.header;
            header[60..64].fill(0);
            let mut payload = Vec::with_capacity(140 + chunk.data.len());
            payload.extend_from_slice(&header);
            payload.extend_from_slice(&chunk.key.kind);
            payload.extend_from_slice(
                &u32::try_from(chunk.data.len() + 76)
                    .map_err(err)?
                    .to_be_bytes(),
            );
            payload.extend_from_slice(&chunk.key.id.to_be_bytes());
            payload.extend_from_slice(&chunk.flags.to_be_bytes());
            payload.extend_from_slice(&chunk.label);
            payload.extend_from_slice(&chunk.data);
            self.add(
                PreparedResource {
                    id: id.clone(),
                    group: source.pack_group.clone(),
                    kind,
                    codec: if chunk.key.kind == *b"TTAB" && source.resolver_variant == "tsbo" {
                        ResourceCodec::IffTtabTsbo
                    } else {
                        ResourceCodec::IffChunk
                    },
                    payload,
                    dependencies: dependencies.into_iter().collect(),
                    simulation_critical: false,
                    locale: None,
                    variants: BTreeSet::new(),
                    provenance: provenance(
                        source,
                        inputs.source_hash,
                        inputs.tuning_hash,
                        inputs.applied.to_vec(),
                    ),
                },
                ResourceMapping {
                    id,
                    source_id: source.id.clone(),
                    filename: source.path.clone(),
                    archive_key: key.clone(),
                    chunk_kind_hex: Some(hex(&chunk.key.kind)),
                    chunk_id: Some(chunk.key.id),
                    classification: kind,
                },
            )?;
        }
        Ok(())
    }
    fn plain(
        &mut self,
        source: &SourceSpec,
        id: String,
        payload: Vec<u8>,
        key: Option<legacy::ResourceKey>,
        hash: &Digest,
    ) -> ManifestResult<()> {
        let (kind, codec) = match source.format {
            SourceFormat::VitaboyAnimation => {
                (ResourceKind::Semantic, ResourceCodec::VitaboyAnimation)
            }
            SourceFormat::VitaboySkeleton => (ResourceKind::Visual, ResourceCodec::VitaboySkeleton),
            SourceFormat::VitaboyMesh => (ResourceKind::Visual, ResourceCodec::VitaboyMesh),
            SourceFormat::VitaboyBinding => (ResourceKind::Visual, ResourceCodec::VitaboyBinding),
            SourceFormat::VitaboyAppearance => {
                (ResourceKind::Visual, ResourceCodec::VitaboyAppearance)
            }
            SourceFormat::VitaboyOutfit => (ResourceKind::Visual, ResourceCodec::VitaboyOutfit),
            SourceFormat::VitaboyPurchasableOutfit => (
                ResourceKind::Visual,
                ResourceCodec::VitaboyPurchasableOutfit,
            ),
            SourceFormat::VitaboyHandGroup => {
                (ResourceKind::Visual, ResourceCodec::VitaboyHandGroup)
            }
            SourceFormat::VitaboyCollection => {
                (ResourceKind::Visual, ResourceCodec::VitaboyCollection)
            }
            SourceFormat::Fsom => (ResourceKind::Visual, ResourceCodec::Fsom),
            SourceFormat::Nbhm => (ResourceKind::Visual, ResourceCodec::Nbhm),
            SourceFormat::PngTexture => (ResourceKind::Visual, ResourceCodec::PngTexture),
            SourceFormat::VitaboyBcf => (ResourceKind::Visual, ResourceCodec::VitaboyBcf),
            SourceFormat::VitaboyCmx => (ResourceKind::Visual, ResourceCodec::VitaboyCmx),
            SourceFormat::VitaboyBmf => (ResourceKind::Visual, ResourceCodec::VitaboyBmf),
            SourceFormat::VitaboySkn => (ResourceKind::Visual, ResourceCodec::VitaboySkn),
            SourceFormat::PcmWave => (ResourceKind::Audio, ResourceCodec::PcmWave),
            SourceFormat::XaMetadata => (ResourceKind::Audio, ResourceCodec::XaMetadata),
            SourceFormat::UtkMetadata => (ResourceKind::Audio, ResourceCodec::UtkMetadata),
            SourceFormat::HitTrackMetadata => {
                (ResourceKind::Audio, ResourceCodec::HitTrackMetadata)
            }
            SourceFormat::HitEventsMetadata => {
                (ResourceKind::Audio, ResourceCodec::HitEventsMetadata)
            }
            SourceFormat::HitHsmMetadata => (ResourceKind::Audio, ResourceCodec::HitHsmMetadata),
            SourceFormat::HitBytecodeMetadata => {
                (ResourceKind::Audio, ResourceCodec::HitBytecodeMetadata)
            }
            SourceFormat::HitlistVersioned => {
                (ResourceKind::Audio, ResourceCodec::HitlistVersioned)
            }
            SourceFormat::HitlistCounted => (ResourceKind::Audio, ResourceCodec::HitlistCounted),
            SourceFormat::HitlistPascalRanges => {
                (ResourceKind::Audio, ResourceCodec::HitlistPascalRanges)
            }
            _ => (ResourceKind::Opaque, ResourceCodec::Opaque),
        };
        let resource = PreparedResource {
            id: id.clone(),
            group: source.pack_group.clone(),
            kind,
            codec,
            payload,
            dependencies: Vec::new(),
            simulation_critical: false,
            locale: None,
            variants: BTreeSet::new(),
            provenance: provenance(source, hash, &self.spec.tuning_version, Vec::new()),
        };
        crate::manifest::validate_payload(&resource, &self.limits.legacy)?;
        self.add(
            resource,
            ResourceMapping {
                id,
                source_id: source.id.clone(),
                filename: source.path.clone(),
                archive_key: key,
                chunk_kind_hex: None,
                chunk_id: None,
                classification: kind,
            },
        )
    }
}
pub fn import_spec(
    spec: &CookSpec,
    root: &Path,
    limits: &CookLimits,
) -> ManifestResult<ImportedContent> {
    spec.validate(limits)?;
    let mut importer = Importer {
        spec,
        root,
        limits,
        read_total: 0,
        decoded_total: 0,
        output: ImportedContent {
            resources: Vec::new(),
            report: ImportReport {
                schema_version: 1,
                fixture_only: false,
                sources: Vec::new(),
                resources: Vec::new(),
                input_hashes: BTreeMap::new(),
                limitations: vec![
                    "Dynamic BHAV references are not automatically discovered; simulation dependency completeness is the caller's explicit accepted contract.".into(),
                    "Opaque entries and PIFF descriptors are preserved but never tick-ready; import performs no nested archive expansion, gameplay execution, automatic mesh conversion or rendering.".into(),
                    "Indexed IFF resource-map edits are unsupported by the content resolver and fail explicitly.".into(),
                    "FSOM import supplies no external replacement texture provider: same-IFF MTEX is the explicit custom-texture fallback, and separate PNG sources do not register renderer replacement precedence.".into(),
                ],
            },
        },
    };
    for source in &spec.sources {
        let bytes = importer.read(&source.path)?;
        let hash = Digest::of(&bytes);
        let name = source.source_name.clone().unwrap_or_else(|| {
            Path::new(&source.path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        });
        match source.format {
            SourceFormat::Iff => importer.iff(source, &source.id, &bytes, name, None, &hash)?,
            SourceFormat::Far1a | SourceFormat::Far1b | SourceFormat::Far3 | SourceFormat::Dbpf => {
                let format = match source.format {
                    SourceFormat::Far1a => legacy::ContainerFormat::Far1a,
                    SourceFormat::Far1b => legacy::ContainerFormat::Far1b,
                    SourceFormat::Far3 => legacy::ContainerFormat::Far3,
                    _ => legacy::ContainerFormat::Dbpf,
                };
                let archive = legacy::index(&bytes, format, &limits.legacy).map_err(err)?;
                let mut extracted_total = 0usize;
                for (index, entry) in archive.entries().iter().enumerate() {
                    extracted_total = extracted_total
                        .checked_add(entry.decoded_len)
                        .ok_or_else(|| err("archive extraction overflow"))?;
                    if extracted_total > limits.legacy.max_total_decoded_bytes {
                        return Err(err("archive extraction budget exceeded"));
                    }
                    let payload = archive.extract(index, &limits.legacy).map_err(err)?;
                    let id = archive_id(&source.id, &entry.key);
                    validate_id(&id, limits.pack.max_id_bytes)?;
                    if payload.starts_with(b"IFF FILE ") {
                        let entry_name = entry.name.as_ref().or(match &entry.key {
                            legacy::ResourceKey::Far1 { name } => Some(name),
                            _ => None,
                        });
                        let exact_name = entry_name
                            .map(|n| std::str::from_utf8(n).map(str::to_string))
                            .transpose()
                            .map_err(err)?
                            .unwrap_or(id.clone());
                        importer.iff(
                            source,
                            &id,
                            &payload,
                            exact_name,
                            Some(entry.key.clone()),
                            &hash,
                        )?;
                    } else {
                        importer.plain(source, id, payload, Some(entry.key.clone()), &hash)?;
                    }
                }
                importer.output.report.sources.push(SourceReport {
                    id: source.id.clone(),
                    source_name: name,
                    original_sha256: hash,
                    resolver_identity: None,
                    applied_patches: Vec::new(),
                    suppressed_patches: Vec::new(),
                });
            }
            _ => {
                importer.plain(source, source.id.clone(), bytes, None, &hash)?;
                importer.output.report.sources.push(SourceReport {
                    id: source.id.clone(),
                    source_name: name,
                    original_sha256: hash,
                    resolver_identity: None,
                    applied_patches: Vec::new(),
                    suppressed_patches: Vec::new(),
                });
            }
        }
    }
    let mut seen = BTreeSet::new();
    for resource in &importer.output.resources {
        validate_id(&resource.id, limits.pack.max_id_bytes)?;
        if !seen.insert(resource.id.clone()) {
            return Err(err("duplicate effective resource key"));
        }
    }
    let positions: BTreeMap<_, _> = importer
        .output
        .resources
        .iter()
        .enumerate()
        .map(|(index, r)| (r.id.clone(), index))
        .collect();
    for o in &spec.overrides {
        let index = *positions
            .get(&o.id)
            .ok_or_else(|| err(format!("unknown override ID {}", o.id)))?;
        let resource = &mut importer.output.resources[index];
        resource.dependencies = resource
            .dependencies
            .iter()
            .chain(&o.dependencies)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        resource.simulation_critical = o.simulation_critical;
        resource.locale = o.locale.clone();
        resource.variants = o.variants.clone();
        if resource.simulation_critical && resource.kind != ResourceKind::Semantic {
            return Err(err("unsupported payload cannot be simulation-critical"));
        }
    }
    for resource in &importer.output.resources {
        for dependency in &resource.dependencies {
            if !seen.contains(dependency) {
                return Err(err(format!("missing dependency {dependency}")));
            }
        }
    }
    Ok(importer.output)
}
/// Authored fixtures are generated from semantic encoders, never copied from installations.
pub fn demo_iff(limits: &legacy::Limits) -> ManifestResult<Vec<u8>> {
    let mut header = [0; 64];
    let signature = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..signature.len()].copy_from_slice(signature);
    let mk = |kind, id, flags, label: &[u8], data| {
        let mut l = [0; 64];
        l[..label.len()].copy_from_slice(label);
        IffChunk {
            key: iff::ChunkKey { kind, id },
            flags,
            label: l,
            data,
        }
    };
    let bhav = semantic::Bhav {
        format_version: 0x8002,
        kind: 0,
        args: 0,
        locals: 0,
        tree_version: 1,
        reserved: vec![0; 2],
        instructions: vec![semantic::BhavInstruction {
            opcode: 2,
            true_pointer: 254,
            false_pointer: 255,
            operand: [0; 8],
        }],
        trailing: Vec::new(),
    };
    let chunks = vec![
        mk(
            *b"BHAV",
            4096,
            0,
            b"Authored BHAV",
            semantic::encode_bhav(&bhav, limits).map_err(err)?,
        ),
        mk(
            *b"BCON",
            4096,
            0,
            b"Authored BCON",
            semantic::encode_bcon(
                &semantic::Bcon {
                    flags: 0,
                    constants: vec![7, 11],
                    trailing: Vec::new(),
                },
                limits,
            )
            .map_err(err)?,
        ),
        mk(
            *b"ZZZZ",
            7,
            0x1234,
            b"Authored ZZZZ",
            b"Authored opaque bytes\0\xff".to_vec(),
        ),
    ];
    iff::encode(&IffFile { header, chunks }, limits).map_err(err)
}
