use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use wonderland_render_core::{AssetKey, derivatives::fsof::Fsof};
use wonderland_world_view::*;
#[path = "support/lighting.rs"]
mod lighting;

fn fixture() -> &'static (WorldDocument, WorldFacadeOutput) {
    static FIXTURE: OnceLock<(WorldDocument, WorldFacadeOutput)> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let world = lighting::lit_world();
        let mut job = WorldFacadeJob::new(Arc::new(world.clone()), Default::default()).unwrap();
        loop {
            if let Some(result) = job.step(128).unwrap() {
                return (world, result);
            }
        }
    })
}
fn digest(bytes: &[u8]) -> AssetKey {
    AssetKey(Sha256::digest(bytes).into())
}
fn verify_with(meta: &str) -> Result<Fsof, WorldError> {
    let (world, output) = fixture();
    verify_world_facade(
        world,
        Default::default(),
        digest(&output.bytes),
        &output.bytes,
        meta,
    )
}
fn changed_receipt(edit: impl FnOnce(&mut Value)) -> String {
    let mut meta: Value = serde_json::from_str(&fixture().1.metadata_json).unwrap();
    edit(&mut meta);
    meta.to_string()
}
#[test]
fn real_export_verifies_against_independent_source_options_and_digest() {
    let (world, output) = fixture();
    let before = world.clone();
    let decoded = verify_with(&output.metadata_json).unwrap();
    let reference = Fsof::decode(&output.bytes, Default::default()).unwrap();
    assert_eq!(decoded, reference);
    assert_eq!(world, &before);
    assert!(decoded.night.is_none());
}
#[test]
fn changed_bytes_cannot_be_blessed_by_rewriting_the_untrusted_receipt() {
    let (world, output) = fixture();
    // Forge another VALID FSOf, not merely a bad gzip checksum. The codec
    // would accept it, so only the independently trusted digest can fence it.
    let mut forged = Fsof::decode(&output.bytes, Default::default()).unwrap();
    forged.floor_texture[0] ^= 1;
    let changed = forged.encode(true, Default::default()).unwrap();
    Fsof::decode(&changed, Default::default()).unwrap();
    let supplied = changed_receipt(|m| {
        m["sha256"] = json!(hex(&digest(&changed).0));
        m["bytes"] = json!(changed.len());
    });
    assert!(
        verify_world_facade(
            world,
            Default::default(),
            digest(&output.bytes),
            &changed,
            &supplied
        )
        .is_err()
    );
    assert!(
        verify_world_facade(
            world,
            Default::default(),
            AssetKey([0; 32]),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn stale_source_revision_provenance_and_options_are_rejected() {
    let (world, output) = fixture();
    let mut changed = world.clone();
    changed.revision.tick += 1;
    assert!(
        verify_world_facade(
            &changed,
            Default::default(),
            digest(&output.bytes),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
    changed = world.clone();
    changed.lighting.as_mut().unwrap().revision += 1;
    assert!(
        verify_world_facade(
            &changed,
            Default::default(),
            digest(&output.bytes),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
    changed = world.clone();
    changed.provenance.origin.push_str("-different-source");
    assert!(
        verify_world_facade(
            &changed,
            Default::default(),
            digest(&output.bytes),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
    assert!(
        verify_world_facade(
            world,
            FacadeExportOptions { pixels_per_tile: 2 },
            digest(&output.bytes),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
}
#[test]
fn receipt_cannot_change_declared_format_scope_identity_or_budget() {
    for (key, value) in [
        ("schema", json!(2)),
        ("kind", json!("game_save")),
        ("not_a_game_save", json!(false)),
        ("format", json!("FSOf anything")),
        ("source_hash", json!("0".repeat(64))),
        ("sha256", json!("0".repeat(64))),
        ("bytes", json!(0)),
        ("regions", json!(0)),
        ("regions", json!(257)),
        ("work_units", json!(100_000_001_u64)),
        ("geometry", json!("invented")),
        ("lighting_state", json!("night invented")),
    ] {
        let meta = changed_receipt(|m| m[key] = value);
        assert!(verify_with(&meta).is_err(), "accepted changed {key}");
    }
}
#[test]
fn receipt_rejects_unknown_duplicate_missing_and_noncanonical_nested_fields() {
    let original = &fixture().1.metadata_json;
    for meta in [
        changed_receipt(|m| m["authority"] = json!("trust-me")),
        changed_receipt(|m| m["provenance"]["authority"] = json!(1)),
        changed_receipt(|m| m["revision"]["epoch_alias"] = json!(1)),
        changed_receipt(|m| m["options"]["night"] = json!(true)),
        changed_receipt(|m| {
            m.as_object_mut().unwrap().remove("sha256");
        }),
        changed_receipt(|m| {
            m["revision"].as_object_mut().unwrap().remove("lot_id");
        }),
        changed_receipt(|m| m["revision"]["tick"] = json!("01")),
        changed_receipt(|m| m["revision"]["tick"] = json!(1)),
        original.replacen("\"schema\":1", "\"schema\":1,\"schema\":1", 1),
        original.replacen("\"tick\":\"1\"", "\"tick\":\"1\",\"tick\":\"1\"", 1),
        format!("{original} false"),
    ] {
        assert!(
            verify_with(&meta).is_err(),
            "accepted invalid receipt: {meta}"
        );
    }
}
#[test]
fn adversarial_diagnostics_and_receipt_sizes_are_bounded() {
    for meta in [
        " ".repeat(65_537),
        changed_receipt(|m| {
            m["diagnostics"] = json!(vec![json!({"code":"a","resource":"b","message":"c"}); 65])
        }),
        changed_receipt(|m| {
            m["diagnostics"] = json!([{"code":"a".repeat(65),"resource":"b","message":"c"}]);
            m["diagnostics_total"] = json!(1);
        }),
        changed_receipt(|m| {
            m["diagnostics"] = json!([{"code":"a","resource":"b","message":"c","authority":true}]);
            m["diagnostics_total"] = json!(1);
        }),
        changed_receipt(|m| {
            m["diagnostics"] = json!([{"code":"a","resource":"b","message":"c"}]);
            m["diagnostics_total"] = json!(0);
        }),
    ] {
        assert!(verify_with(&meta).is_err());
    }
}
#[test]
fn trusted_checksum_still_requires_a_supported_bounded_fsof_decode() {
    let (world, output) = fixture();
    let mut raw = output.bytes.clone();
    raw[8] = 0;
    let mut version = output.bytes.clone();
    version[4] = 2;
    let mut trailing = output.bytes.clone();
    trailing.push(0);
    for bytes in [
        vec![],
        b"FSOf\x01\0\0\0\x01".to_vec(),
        raw,
        version,
        trailing,
        vec![0; 16 * 1024 * 1024 + 1],
    ] {
        let hash = digest(&bytes);
        let meta = changed_receipt(|m| {
            m["sha256"] = json!(hex(&hash.0));
            m["bytes"] = json!(bytes.len());
        });
        assert!(verify_world_facade(world, Default::default(), hash, &bytes, &meta).is_err());
    }
}
#[test]
fn invalid_expected_source_is_rejected_even_with_a_matching_export_digest() {
    let (world, output) = fixture();
    let mut invalid = world.clone();
    invalid.lighting.as_mut().unwrap().geometry[0].walls[0][0].x = f32::NAN;
    assert!(
        verify_world_facade(
            &invalid,
            Default::default(),
            digest(&output.bytes),
            &output.bytes,
            &output.metadata_json
        )
        .is_err()
    );
    for pixels_per_tile in [0, 9, u16::MAX] {
        assert!(
            verify_world_facade(
                world,
                FacadeExportOptions { pixels_per_tile },
                digest(&output.bytes),
                &output.bytes,
                &output.metadata_json
            )
            .is_err()
        );
    }
}

#[test]
fn receiving_byte_caps_run_before_hashing_or_receipt_parsing() {
    let (world, output) = fixture();
    let mut oversized = vec![0; 16 * 1024 * 1024 + 1];
    oversized[..9].copy_from_slice(b"FSOf\x01\0\0\0\x01");
    // Deliberately wrong trusted digest: a missing preflight would reach the
    // digest mismatch instead. Require the preflight error, not any error.
    let error = verify_world_facade(
        world,
        Default::default(),
        AssetKey([0; 32]),
        &oversized,
        &output.metadata_json,
    )
    .unwrap_err();
    assert!(error.0.contains("input byte budget"));
    let error = verify_world_facade(
        world,
        Default::default(),
        digest(&output.bytes),
        &output.bytes,
        &" ".repeat(65_537),
    )
    .unwrap_err();
    assert!(error.0.contains("input byte budget"));
}

#[test]
fn otherwise_valid_night_and_bc3_payloads_cannot_misdeclare_rgba8_single_state() {
    use wonderland_render_core::derivatives::fsof::{FsofNight, TextureCompression};
    let (world, output) = fixture();
    let mut night = Fsof::decode(&output.bytes, Default::default()).unwrap();
    night.night = Some(FsofNight {
        floor_texture: night.floor_texture.clone(),
        wall_texture: night.wall_texture.clone(),
        light_color: [0; 4],
    });
    let mut bc3 = Fsof::decode(&output.bytes, Default::default()).unwrap();
    bc3.compression = TextureCompression::Dxt5;
    bc3.floor_texture.resize(
        (bc3.floor_width.div_ceil(4) * bc3.floor_height.div_ceil(4) * 16) as usize,
        0,
    );
    bc3.wall_texture.resize(
        (bc3.wall_width.div_ceil(4) * bc3.wall_height.div_ceil(4) * 16) as usize,
        0,
    );
    for value in [night, bc3] {
        let bytes = value.encode(true, Default::default()).unwrap();
        Fsof::decode(&bytes, Default::default()).unwrap();
        let hash = digest(&bytes);
        let meta = changed_receipt(|m| {
            m["sha256"] = json!(hex(&hash.0));
            m["bytes"] = json!(bytes.len());
        });
        let error =
            verify_world_facade(world, Default::default(), hash, &bytes, &meta).unwrap_err();
        assert!(error.0.contains("single RGBA8 light state"));
    }
}
