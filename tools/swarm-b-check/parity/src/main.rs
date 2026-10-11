// SPDX-License-Identifier: MPL-2.0
//! Test-only native/WASI execution probe; no VM, replay, or rendering claim.
#![forbid(unsafe_code)]
mod fixtures;
#[path = "../../../../tests/interactions/fixtures.rs"]
mod interaction_fixture;

use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::{
    manifest::*,
    objects::{sha256_hex, SourceContent},
    packs::*,
    patches::PatchFile,
    resolve_content,
    tuning::*,
    tuning_pack, ResolveRequest,
};
use wonderland_interactions_check as interactions;
use wonderland_legacy_formats::{compression::*, iff::*, semantic::*, vitaboy::*, Limits};

const BASELINE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
fn chunk(kind: [u8; 4], id: u16, data: &[u8]) -> IffChunk {
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0x1234,
        label: [0; 64],
        data: data.into(),
    }
}
fn file(chunks: Vec<IffChunk>) -> IffFile {
    let mut header = [0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..magic.len()].copy_from_slice(magic);
    IffFile { header, chunks }
}
fn utf8(s: &str) -> LegacyString {
    LegacyString::from_text(s, TextEncoding::Utf8).unwrap()
}
fn patch(name: &str, user: bool, bytes: &[u8]) -> PatchFile {
    let piff = Piff {
        version: 2,
        source: utf8("authored.iff"),
        comment: utf8(""),
        trailing: vec![],
        entries: vec![PiffEntry {
            kind: *b"BCON",
            id: 4096,
            comment: utf8(""),
            operation: PiffOperation::Patch {
                label: utf8(""),
                flags: 0xf00d,
                new_id: 4096,
                new_size: bytes.len() as u32,
                patches: vec![
                    PiffPatch {
                        offset: 0,
                        size: fixtures::BCON.len() as u32,
                        mode: PiffPatchMode::Remove,
                        data: vec![],
                    },
                    PiffPatch {
                        offset: 0,
                        size: bytes.len() as u32,
                        mode: PiffPatchMode::Add,
                        data: bytes.into(),
                    },
                ],
            },
        }],
    };
    PatchFile {
        name: name.into(),
        is_user: user,
        file: file(vec![chunk(
            *b"PIFF",
            256,
            &encode_piff(&piff, &Limits::default()).unwrap(),
        )]),
    }
}

fn content() -> (Value, Vec<u8>, Vec<u8>) {
    let limits = Limits::default();
    let raw = decompress_refpack(fixtures::REFPACK, 10, &limits).unwrap();
    let qfs = decompress_qfs(fixtures::QFS, &limits).unwrap();
    assert_eq!(raw, b"abcdabcd!?");
    assert_eq!(qfs, b"abcdabcd!?");
    assert!(decompress_refpack(&[0, 0, 0xfc], 3, &limits).is_err());
    let bhav = decode_bhav(fixtures::BHAV, &limits).unwrap();
    assert_eq!(bhav.locals, 0x1234);
    assert_eq!(bhav.instructions[0].opcode, 0x1234);
    assert_eq!(bhav.instructions[0].true_pointer, 254);
    assert_eq!(bhav.instructions[0].false_pointer, 255);
    assert_eq!(bhav.instructions[0].operand, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(bhav.trailing, [0xde, 0xad]);
    assert_eq!(encode_bhav(&bhav, &limits).unwrap(), fixtures::BHAV);
    let bcon = decode_bcon(fixtures::BCON, &limits).unwrap();
    assert_eq!(bcon.constants, [7, 65535]);
    assert_eq!(bcon.flags, 0xa5);
    assert_eq!(bcon.trailing, [0xde, 0xad]);
    assert_eq!(encode_bcon(&bcon, &limits).unwrap(), fixtures::BCON);
    let mut request = ResolveRequest::new(
        "authored.iff",
        file(vec![
            chunk(*b"BHAV", 4096, fixtures::BHAV),
            chunk(*b"BCON", 4096, fixtures::BCON),
            chunk(*b"UNKN", 77, &[0xff, 0, 0x80]),
        ]),
    );
    request.semiglobal = Some(SourceContent {
        name: "semi.iff".into(),
        iff: file(vec![chunk(*b"BCON", 8192, fixtures::BCON)]),
    });
    request.global = Some(SourceContent {
        name: "global.iff".into(),
        iff: file(vec![chunk(*b"BCON", 256, fixtures::BCON)]),
    });
    request.patches = vec![
        patch("base", false, &[2, 0xa5, 10, 0, 0xff, 0xff, 0xde, 0xad]),
        patch("user-a", true, &[2, 0xa5, 20, 0, 0xff, 0xff, 0xde, 0xad]),
        patch("user-b", true, &[2, 0xa5, 30, 0, 0xff, 0xff, 0xde, 0xad]),
    ];
    request.tuning = TuningInputs {
        otf: Some(decode_otf(fixtures::OTF, &limits).unwrap()),
        upgrades: vec![TuningOverride {
            table: 8192,
            index: 1,
            value: -42,
        }],
        dynamic_private: vec![DynamicOverride {
            table: 0,
            index: 0,
            value_bits: 400.9f32.to_bits(),
        }],
        dynamic_semiglobal: vec![DynamicOverride {
            table: 0,
            index: 0,
            value_bits: 500.9f32.to_bits(),
        }],
        ..TuningInputs::default()
    };
    let source_before = request.source.iff.clone();
    let resolved = resolve_content(&request, &limits).unwrap();
    assert_eq!(request.source.iff, source_before);
    assert_eq!(
        resolved
            .applied_patches
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["user-a", "user-b"]
    );
    assert_eq!(
        resolved
            .applied_patches
            .iter()
            .map(|p| p.order)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(resolved.suppressed_patches.len(), 1);
    let patched = resolved
        .iff
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BCON")
        .unwrap();
    assert_eq!(patched.data, [2, 0xa5, 30, 0, 0xff, 0xff, 0xde, 0xad]);
    assert_eq!(patched.flags, 0x1234);
    assert_eq!(
        resolved.scopes().lookup_bhav(4096).unwrap().data,
        fixtures::BHAV
    );
    assert!(resolved.scopes().lookup_bhav(8192).is_none());
    assert_eq!(
        resolved
            .iff
            .chunks
            .iter()
            .find(|c| c.key.kind == *b"UNKN")
            .unwrap()
            .data,
        [0xff, 0, 0x80]
    );
    let mut reverse = request.clone();
    reverse.patches.swap(1, 2);
    let reversed = resolve_content(&reverse, &limits).unwrap();
    assert_ne!(reversed.identity, resolved.identity);
    assert_eq!(
        decode_bcon(&reversed.iff.chunks[1].data, &limits)
            .unwrap()
            .constants[0],
        20
    );
    assert_eq!(
        resolve_content(&request, &limits).unwrap().identity,
        resolved.identity
    );
    let selected = [0, 1, 64 << 7, (64 << 7) | 1, 128 << 7, 9];
    assert_eq!(
        selected.map(|i| resolved.tuning.value_or_zero(i)),
        [400, -1, 500, -42, 7, 0]
    );
    assert_eq!(
        resolved.tuning.lookup_encoded(0).origin,
        TuningOrigin::DynamicPrivate
    );
    assert_eq!(
        resolved.tuning.lookup_encoded(64 << 7).origin,
        TuningOrigin::DynamicSemiglobal
    );
    assert_eq!(
        resolved.tuning.lookup_encoded(9).origin,
        TuningOrigin::Missing
    );
    let tuning_bytes = tuning_pack::encode(&resolved.tuning, &limits).unwrap();
    assert_eq!(&tuning_bytes[..8], b"WLTUNE\0\0");
    let portable = tuning_pack::decode(&tuning_bytes, &limits).unwrap();
    assert_eq!(portable, resolved.tuning);
    let mut lookup_hash = Sha256::new();
    for operand in 0..=u16::MAX {
        let expected = resolved.tuning.lookup_encoded(operand);
        assert_eq!(portable.lookup_encoded(operand), expected);
        lookup_hash.update(serde_json::to_vec(&expected).unwrap());
        lookup_hash.update(b"\n");
    }
    assert_eq!(
        tuning_pack::encode(&portable, &limits).unwrap(),
        tuning_bytes
    );
    let animation_bytes = fixtures::animation();
    let animation = decode_animation(&animation_bytes, &limits).unwrap();
    assert_eq!(animation.version, 2);
    assert_eq!(animation.duration_ms.0, 0x447a0000);
    assert_eq!(animation.distance.0, 0x40200000);
    assert_eq!(
        animation.translations[0].map(|v| v.0),
        [0xbf800000, 0x80000000, 0x40400000]
    );
    assert_eq!(
        animation.rotations[0].map(|v| v.0),
        [0, 0x80000000, 0x80000000, 0xbf800000]
    );
    assert_eq!(animation.num_frames, 1);
    assert_eq!(animation.frames_per_second(), Some(1));
    (
        json!({"refpack": raw, "qfs": qfs, "bhav": bhav, "bcon": bcon,
        "resolved": {"identity": sha256_hex(&resolved.identity), "source_hash": sha256_hex(&resolved.source_hash),
            "reversed_identity": sha256_hex(&reversed.identity), "chunks": resolved.iff.chunks.iter().map(|c| json!({"key": c.key, "flags": c.flags, "label": c.label.to_vec(), "data": c.data})).collect::<Vec<_>>(),
            "semantic_resources": resolved.semantic_resources, "resources": resolved.resources,
            "applied": resolved.applied_patches, "suppressed": resolved.suppressed_patches, "scopes": resolved.scope_provenance},
        "tuning": {"bytes": tuning_bytes, "selected": selected.map(|i| portable.lookup_encoded(i)),
            "identity": sha256_hex(&portable.identity), "all_65536_lookups_sha256": format!("{:x}", lookup_hash.finalize())},
        "animation": animation}),
        tuning_bytes,
        animation_bytes,
    )
}

fn packs(tuning: Vec<u8>, animation: Vec<u8>) -> Value {
    let limits = ManifestLimits::default();
    let pack_limits = PackLimits::default();
    let resources = vec![
        PackResource {
            id: "tuning".into(),
            kind: ResourceKind::Semantic,
            bytes: tuning,
        },
        PackResource {
            id: "animation".into(),
            kind: ResourceKind::Visual,
            bytes: animation,
        },
        PackResource {
            id: "unselected".into(),
            kind: ResourceKind::Opaque,
            bytes: b"authored unrelated".to_vec(),
        },
    ];
    let bytes = build_pack(&resources, &pack_limits).unwrap();
    let mut reversed = resources.clone();
    reversed.reverse();
    assert_eq!(build_pack(&reversed, &pack_limits).unwrap(), bytes);
    assert_eq!(&bytes[..8], b"WLDPACK\0");
    let hash = Digest::of(&bytes);
    let index = verify_pack(&bytes, &hash, &pack_limits).unwrap();
    assert_eq!(
        index
            .entries()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["animation", "tuning", "unselected"]
    );
    let mut records = BTreeMap::new();
    for r in &resources {
        assert_eq!(index.get(&r.id).unwrap(), r.bytes);
        records.insert(
            r.id.clone(),
            ResourceRecord {
                content_hash: Digest::of(&r.bytes),
                pack: hash.clone(),
                kind: r.kind,
                codec: match r.id.as_str() {
                    "tuning" => ResourceCodec::ResolvedTuning,
                    "animation" => ResourceCodec::VitaboyAnimation,
                    _ => ResourceCodec::Opaque,
                },
                dependencies: if r.id == "animation" {
                    vec!["tuning".into()]
                } else {
                    vec![]
                },
                simulation_critical: r.id == "tuning",
                locale: None,
                variants: BTreeSet::new(),
                provenance: Provenance {
                    origin: Origin::Authored,
                    source: "parity fixtures".into(),
                    source_hash: Digest::of(b"authored parity fixture v1"),
                    patch_hashes: vec![],
                    tuning_hash: None,
                    license: Some("MPL-2.0".into()),
                    redistribution: Redistribution::Allowed,
                },
            },
        );
    }
    let manifest = AssetManifest {
        schema_version: MANIFEST_VERSION,
        pack_format_version: PACK_VERSION,
        source_baseline: BASELINE.into(),
        content_version: Digest::of(b""),
        tuning_version: records["tuning"].content_hash.clone(),
        resources: records,
        packs: BTreeMap::from([(
            hash.clone(),
            PackRecord {
                byte_len: bytes.len() as u64,
                format_version: PACK_VERSION,
                resources: vec!["animation".into(), "tuning".into(), "unselected".into()],
            },
        )]),
    }
    .seal(&limits)
    .unwrap();
    index.validate_manifest(&manifest, &limits).unwrap();
    manifest.require_public_distribution().unwrap();
    let roots = vec!["animation".into()];
    let plan = manifest
        .load_plan(&roots, LoadPhase::All, None, &BTreeSet::new(), &limits)
        .unwrap();
    assert_eq!(plan.resources, ["tuning", "animation"]);
    assert_eq!(plan.packs.as_slice(), std::slice::from_ref(&hash));
    assert_eq!(plan.download_bytes, bytes.len() as u64);
    let simulation = manifest
        .load_plan(
            &roots,
            LoadPhase::Simulation,
            None,
            &BTreeSet::new(),
            &limits,
        )
        .unwrap();
    assert_eq!(simulation.resources, ["tuning"]);
    let cached = manifest
        .load_plan(
            &roots,
            LoadPhase::All,
            None,
            &BTreeSet::from([hash.clone()]),
            &limits,
        )
        .unwrap();
    assert!(cached.packs.is_empty());
    assert_eq!(cached.download_bytes, 0);
    let canonical = manifest.canonical_bytes(&limits).unwrap();
    let manifest_hash = Digest::of(&canonical);
    assert_eq!(
        AssetManifest::from_json_verified(&canonical, &manifest_hash, &limits).unwrap(),
        manifest
    );
    let mut tampered = bytes.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(verify_pack(&tampered, &hash, &pack_limits).is_err());
    assert!(AssetManifest::from_json_verified(&canonical, &Digest::of(b"wrong"), &limits).is_err());
    json!({"bytes": bytes, "pack_hash": hash, "manifest": manifest,
        "canonical_manifest_bytes": canonical, "manifest_hash": manifest_hash,
        "subset_plan": plan, "simulation_plan": simulation, "cached_plan": cached})
}

fn query() -> Value {
    use interaction_fixture::*;
    let world = FixtureWorld::new(vec![definition(300)]);
    let before = world.clone();
    let checks = FixtureChecks {
        expected_origin: Some(interactions::CheckOrigin::Ui),
        ..Default::default()
    };
    let result = interactions::query_offers(
        &world,
        &checks,
        world.query(),
        &interactions::InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result,
        interactions::query_offers(
            &world,
            &checks,
            world.query(),
            &interactions::InteractionLimits::default()
        )
        .unwrap()
    );
    assert_eq!(world, before);
    assert_eq!(result.offers.len(), 1);
    assert_eq!(result.offers[0].interaction.tta_index, 300);
    assert_eq!(result.offers[0].param0, 0);
    assert_eq!(result.offers[0].label, "Fixture 300");
    assert_eq!(
        result.offers[0].advertisements,
        [
            interactions::AdvertisementChange {
                motive: 2,
                value: 3
            },
            interactions::AdvertisementChange {
                motive: 7,
                value: 15
            }
        ]
    );
    json!({"provider": "authored snapshot/check fixture; no BHAV VM", "world_unchanged": world == before,
        "seen_world_revision": result.seen.world_revision,
        "offers": result.offers.iter().map(|o| json!({"tta_index": o.interaction.tta_index,
            "scope": format!("{:?}", o.interaction.scope), "param0": o.param0, "label": o.label,
            "advertisements": o.advertisements.iter().map(|a| json!({"motive": a.motive, "value": a.value})).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "live_rng": world.live.state().rng_seed, "live_temps": world.live.state().temp_registers,
        "live_temp_xl": world.live.state().temp_xl, "live_provider_bytes": world.live.state().provider_state()})
}
fn main() {
    let (content, tuning, animation) = content();
    let result = json!({"schema_version": 1, "source_baseline": BASELINE,
        "content": content, "immutable_pack_manifest": packs(tuning, animation), "isolated_ui_query": query()});
    println!("{}", serde_json::to_string(&result).unwrap());
}
