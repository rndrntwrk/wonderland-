// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
//! Construct demand-grouped content sets from validated, source-resolved resources.
use crate::packs::{build_pack, verify_pack, PackLimits, PackResource};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::manifest::*;

#[derive(Clone, Debug)]
pub struct PreparedResource {
    pub id: String,
    pub group: String,
    pub kind: ResourceKind,
    pub codec: ResourceCodec,
    pub payload: Vec<u8>,
    pub dependencies: Vec<String>,
    pub simulation_critical: bool,
    pub locale: Option<String>,
    pub variants: BTreeSet<String>,
    pub provenance: Provenance,
}
#[derive(Clone, Debug, Default)]
pub struct CookLimits {
    pub manifest: ManifestLimits,
    pub pack: PackLimits,
    pub legacy: wonderland_legacy_formats::Limits,
}
#[derive(Debug)]
pub struct CookedContent {
    pub manifest: AssetManifest,
    pub packs: BTreeMap<Digest, Vec<u8>>,
}

pub fn cook_resources(
    source_baseline: &str,
    tuning_version: Digest,
    resources: &[PreparedResource],
    limits: &CookLimits,
) -> ManifestResult<CookedContent> {
    if resources.is_empty() || resources.len() > limits.manifest.max_resources {
        return Err(ManifestError("invalid cooked resource count".into()));
    }
    let mut groups: BTreeMap<String, BTreeMap<&str, &PreparedResource>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total_input = 0usize;
    for resource in resources {
        validate_id(&resource.id, limits.pack.max_id_bytes)?;
        validate_id(&resource.group, limits.manifest.max_string_bytes)?;
        if !seen.insert(&resource.id) {
            return Err(ManifestError("duplicate effective resource ID".into()));
        }
        if resource.payload.len() > limits.pack.max_resource_bytes {
            return Err(ManifestError("cooked resource byte limit exceeded".into()));
        }
        total_input = total_input
            .checked_add(resource.payload.len())
            .ok_or_else(|| ManifestError("cooker input size overflow".into()))?;
        if total_input > limits.legacy.max_total_decoded_bytes {
            return Err(ManifestError(
                "cooker aggregate byte budget exceeded".into(),
            ));
        }
        validate_payload(resource, &limits.legacy)?;
        let group = format!("{}/{:?}", resource.group, resource.kind);
        groups
            .entry(group)
            .or_default()
            .insert(&resource.id, resource);
    }
    let mut result = CookedContent {
        manifest: AssetManifest {
            schema_version: MANIFEST_VERSION,
            pack_format_version: PACK_VERSION,
            source_baseline: source_baseline.into(),
            content_version: Digest::of(&[]),
            tuning_version,
            packs: BTreeMap::new(),
            resources: BTreeMap::new(),
        },
        packs: BTreeMap::new(),
    };
    for group in groups.values() {
        let mut batch: Vec<&PreparedResource> = Vec::new();
        let mut size = crate::packs::HEADER_LEN;
        for resource in group.values() {
            let increment = 52usize
                .checked_add(resource.id.len())
                .and_then(|n| n.checked_add(resource.payload.len()))
                .ok_or_else(|| ManifestError("pack size overflow".into()))?;
            if !batch.is_empty()
                && (size
                    .checked_add(increment)
                    .is_none_or(|n| n > limits.pack.max_pack_bytes)
                    || batch.len() >= limits.pack.max_resources)
            {
                append_pack(&mut result, &batch, limits)?;
                batch.clear();
                size = crate::packs::HEADER_LEN;
            }
            size = size
                .checked_add(increment)
                .ok_or_else(|| ManifestError("pack size overflow".into()))?;
            if size > limits.pack.max_pack_bytes {
                return Err(ManifestError(
                    "one resource cannot fit the configured pack budget".into(),
                ));
            }
            batch.push(resource);
        }
        if !batch.is_empty() {
            append_pack(&mut result, &batch, limits)?;
        }
    }
    result.manifest = result.manifest.seal(&limits.manifest)?;
    for (hash, bytes) in &result.packs {
        verify_pack(bytes, hash, &limits.pack)?
            .validate_manifest(&result.manifest, &limits.manifest)?;
    }
    Ok(result)
}

fn append_pack(
    output: &mut CookedContent,
    batch: &[&PreparedResource],
    limits: &CookLimits,
) -> ManifestResult<()> {
    if output.packs.len() >= limits.manifest.max_packs {
        return Err(ManifestError("cooked pack count limit exceeded".into()));
    }
    let input: Vec<_> = batch
        .iter()
        .map(|resource| PackResource {
            id: resource.id.clone(),
            kind: resource.kind,
            bytes: resource.payload.clone(),
        })
        .collect();
    let bytes = build_pack(&input, &limits.pack)?;
    let hash = Digest::of(&bytes);
    let current_total: u64 = output.manifest.packs.values().map(|p| p.byte_len).sum();
    if current_total
        .checked_add(bytes.len() as u64)
        .is_none_or(|n| n > limits.manifest.max_download_bytes)
    {
        return Err(ManifestError(
            "cooked release aggregate output budget exceeded".into(),
        ));
    }
    let ids = batch.iter().map(|r| r.id.clone()).collect();
    output.manifest.packs.insert(
        hash.clone(),
        PackRecord {
            byte_len: bytes.len() as u64,
            format_version: PACK_VERSION,
            resources: ids,
        },
    );
    for resource in batch {
        output.manifest.resources.insert(
            resource.id.clone(),
            ResourceRecord {
                content_hash: Digest::of(&resource.payload),
                pack: hash.clone(),
                kind: resource.kind,
                codec: resource.codec,
                dependencies: resource.dependencies.clone(),
                simulation_critical: resource.simulation_critical,
                locale: resource.locale.clone(),
                variants: resource.variants.clone(),
                provenance: resource.provenance.clone(),
            },
        );
    }
    output.packs.insert(hash, bytes);
    Ok(())
}

/// Semantic bytes are decoded before a caller can declare them ready for a tick.
pub fn validate_payload(
    resource: &PreparedResource,
    limits: &wonderland_legacy_formats::Limits,
) -> ManifestResult<()> {
    use wonderland_legacy_formats::{audio_meta, iff, reconstruction, semantic, textures, vitaboy};
    let map = |e: wonderland_legacy_formats::Error| ManifestError(e.to_string());
    match resource.codec {
        ResourceCodec::IffChunk | ResourceCodec::IffTtabTsbo => {
            let file = iff::decode(&resource.payload, limits).map_err(map)?;
            if file.chunks.len() != 1 {
                return Err(ManifestError(
                    "IFF resource pack member must contain exactly one chunk".into(),
                ));
            }
            if resource.codec == ResourceCodec::IffTtabTsbo {
                if resource.kind != ResourceKind::Semantic || file.chunks[0].key.kind != *b"TTAB" {
                    return Err(ManifestError(
                        "TSBO TTAB codec requires one semantic TTAB chunk".into(),
                    ));
                }
                semantic::decode_ttab_with_variant(
                    &file.chunks[0].data,
                    semantic::TtabVariant::Tsbo,
                    limits,
                )
                .map_err(map)?;
            } else if resource.kind == ResourceKind::Semantic {
                let chunk = &file.chunks[0];
                match semantic::decode_semantic(chunk, limits).map_err(map)? {
                    semantic::DecodedSemantic::Unknown { .. }
                    | semantic::DecodedSemantic::Piff(_) => {
                        return Err(ManifestError("unknown chunks and unapplied patch streams cannot be effective simulation semantics".into()));
                    }
                    _ => {}
                }
            }
            if resource.kind == ResourceKind::Visual {
                match &file.chunks[0].key.kind {
                    b"FSOM" => {
                        reconstruction::decode_fsom(&file.chunks[0].data, limits).map_err(map)?;
                    }
                    b"MTEX" => {
                        textures::decode_png(&file.chunks[0].data, limits).map_err(map)?;
                    }
                    _ => {}
                }
            }
        }
        ResourceCodec::ResolvedTuning => {
            if resource.kind != ResourceKind::Semantic {
                return Err(ManifestError(
                    "effective tuning must have semantic resource kind".into(),
                ));
            }
            wonderland_content_ir::tuning_pack::decode(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboyAnimation => {
            vitaboy::decode_animation(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboySkeleton => {
            vitaboy::decode_skeleton(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboyMesh => {
            vitaboy::decode_mesh(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboyBinding => {
            vitaboy::decode_binding(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboyAppearance => {
            vitaboy::decode_appearance(&resource.payload, limits).map_err(map)?;
        }
        ResourceCodec::VitaboyOutfit => {
            vitaboy::decode_outfit(&resource.payload, limits).map_err(map)?;
        }
        codec @ (ResourceCodec::VitaboyPurchasableOutfit
        | ResourceCodec::VitaboyHandGroup
        | ResourceCodec::VitaboyCollection
        | ResourceCodec::VitaboyBcf
        | ResourceCodec::VitaboyCmx
        | ResourceCodec::VitaboyBmf
        | ResourceCodec::VitaboySkn
        | ResourceCodec::Fsom
        | ResourceCodec::Nbhm
        | ResourceCodec::PngTexture) => {
            if resource.kind != ResourceKind::Visual || resource.simulation_critical {
                return Err(ManifestError(
                    "visual codecs require noncritical visual resource kind".into(),
                ));
            }
            match codec {
                ResourceCodec::VitaboyPurchasableOutfit => {
                    vitaboy::decode_purchasable_outfit(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::VitaboyHandGroup => {
                    vitaboy::decode_hand_group(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::VitaboyCollection => {
                    vitaboy::decode_collection(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::VitaboyBcf => {
                    vitaboy::decode_bcf(&resource.payload, vitaboy::LegacyEncoding::Binary, limits)
                        .map_err(map)?;
                }
                ResourceCodec::VitaboyCmx => {
                    vitaboy::decode_bcf(&resource.payload, vitaboy::LegacyEncoding::Text, limits)
                        .map_err(map)?;
                }
                ResourceCodec::VitaboyBmf => {
                    vitaboy::decode_bmf(&resource.payload, vitaboy::LegacyEncoding::Binary, limits)
                        .map_err(map)?;
                }
                ResourceCodec::VitaboySkn => {
                    vitaboy::decode_bmf(&resource.payload, vitaboy::LegacyEncoding::Text, limits)
                        .map_err(map)?;
                }
                ResourceCodec::Fsom => {
                    reconstruction::decode_fsom(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::Nbhm => {
                    reconstruction::decode_nbhm(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::PngTexture => {
                    textures::decode_png(&resource.payload, limits).map_err(map)?;
                }
                _ => unreachable!("outer match restricts codec"),
            }
        }
        codec @ (ResourceCodec::PcmWave
        | ResourceCodec::XaMetadata
        | ResourceCodec::UtkMetadata
        | ResourceCodec::HitTrackMetadata
        | ResourceCodec::HitEventsMetadata
        | ResourceCodec::HitHsmMetadata
        | ResourceCodec::HitBytecodeMetadata
        | ResourceCodec::HitlistVersioned
        | ResourceCodec::HitlistCounted
        | ResourceCodec::HitlistPascalRanges) => {
            if resource.kind != ResourceKind::Audio || resource.simulation_critical {
                return Err(ManifestError(
                    "audio codecs require noncritical audio resource kind".into(),
                ));
            }
            match codec {
                ResourceCodec::PcmWave => {
                    audio_meta::decode_wave(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::XaMetadata => {
                    audio_meta::decode_xa(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::UtkMetadata => {
                    audio_meta::decode_utk(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::HitTrackMetadata => {
                    audio_meta::decode_track(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::HitEventsMetadata => {
                    audio_meta::decode_events(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::HitHsmMetadata => {
                    audio_meta::decode_hsm(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::HitBytecodeMetadata => {
                    audio_meta::decode_hit_metadata(&resource.payload, limits).map_err(map)?;
                }
                ResourceCodec::HitlistVersioned
                | ResourceCodec::HitlistCounted
                | ResourceCodec::HitlistPascalRanges => {
                    let encoding = match codec {
                        ResourceCodec::HitlistVersioned => {
                            audio_meta::HitlistEncoding::VersionedBinary
                        }
                        ResourceCodec::HitlistCounted => audio_meta::HitlistEncoding::CountedBinary,
                        _ => audio_meta::HitlistEncoding::PascalRanges,
                    };
                    audio_meta::decode_hitlist(&resource.payload, encoding, limits).map_err(map)?;
                }
                _ => unreachable!("non-audio codec excluded by outer match"),
            }
        }
        ResourceCodec::Opaque => {
            if resource.kind != ResourceKind::Opaque {
                return Err(ManifestError(
                    "opaque bytes cannot assert a semantic, visual or audio decoder".into(),
                ));
            }
        }
    }
    Ok(())
}
