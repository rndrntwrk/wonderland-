use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
use wonderland_creator::{
    default_limits,
    editors::sprites::{SpritePackage, MAX_SPRITE_PACKAGE_BYTES},
    sha256, ResourceDocument,
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey},
    sprites::{decode_palt, decode_spr2_with_palettes, encode_palt, Spr2AlphaMode},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "creator-sprite-authoring-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
    fn read(&self, name: &str) -> Vec<u8> {
        fs::read(self.0.join(name)).unwrap()
    }
    fn json(&self, name: &str) -> Value {
        serde_json::from_slice(&self.read(name)).unwrap()
    }
    fn put_json(&self, name: &str, value: &Value) {
        self.write(name, serde_json::to_vec(value).unwrap());
    }
    fn call(&self, args: &[&str], success: bool) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_creator"))
            .arg("--root")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{args:?}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !success {
            assert_eq!(output.status.code(), Some(2));
        }
        output
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn chunk(kind: &[u8; 4], id: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    bytes.extend_from_slice(&(76u32 + payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(&0x1234u16.to_be_bytes());
    let mut label = [0x99; 64];
    label[..5].copy_from_slice(b"part\0");
    bytes.extend_from_slice(&label);
    bytes.extend_from_slice(payload);
    bytes
}

fn fixture(version: u32, indexed: bool) -> Vec<u8> {
    let mut palette = 1u32.to_le_bytes().to_vec();
    palette.extend_from_slice(&3u32.to_le_bytes());
    palette.extend_from_slice(&[0xa3; 8]);
    palette.extend_from_slice(&[11, 12, 13, 21, 22, 23, 31, 32, 33]);
    // Independent source command vector: odd color+padding, alpha/depth,
    // opaque depth, transparency, skipped row, end. Nonzero padding and extra
    // zero tail distinguish exact passthrough from canonical re-encoding.
    let commands = [
        18, 0, 1, 0xc0, 1, 0xe7, 1, 0x40, 19, 2, 15, 0xd2, 1, 0x20, 9, 1, 1, 0x60, 1, 0x80, 0,
        0xa0, 0, 0,
    ];
    let mut frame = 4u16.to_le_bytes().to_vec();
    frame.extend_from_slice(&2u16.to_le_bytes());
    frame.extend_from_slice(&7u32.to_le_bytes());
    frame.extend_from_slice(&0u16.to_le_bytes());
    frame.extend_from_slice(&0u16.to_le_bytes());
    frame.extend_from_slice(&(-2i16).to_le_bytes());
    frame.extend_from_slice(&5i16.to_le_bytes());
    frame.extend_from_slice(&commands);
    let mut sprite = version.to_le_bytes().to_vec();
    if version == 1000 {
        sprite.extend_from_slice(&1u32.to_le_bytes());
        sprite.extend_from_slice(&0x12340007u32.to_le_bytes());
        sprite.extend_from_slice(&16u32.to_le_bytes());
    } else {
        sprite.extend_from_slice(&7u32.to_le_bytes());
        sprite.extend_from_slice(&1u32.to_le_bytes());
        sprite.extend_from_slice(&1001u32.to_le_bytes());
        sprite.extend_from_slice(&(frame.len() as u32).to_le_bytes());
    }
    sprite.extend_from_slice(&frame);
    let mut bytes = vec![0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1";
    bytes[..magic.len()].copy_from_slice(magic);
    let mut map = vec![0; 8];
    map.extend_from_slice(b"pmsr");
    map.extend_from_slice(&0u32.to_le_bytes());
    map.extend_from_slice(&3u32.to_le_bytes());
    for (kind, id, payload) in [
        (*b"PALT", 7u16, palette),
        (*b"SPR2", 8, sprite),
        (*b"ZZZZ", 9, vec![0, 255, 7, 3, 11]),
    ] {
        map.extend(kind.iter().rev());
        map.extend_from_slice(&1u32.to_le_bytes());
        map.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        map.extend_from_slice(&id.to_le_bytes());
        map.extend_from_slice(&0x1234u16.to_le_bytes());
        map.extend_from_slice(b"part\0\xa6");
        bytes.extend_from_slice(&chunk(&kind, id, &payload));
    }
    if indexed {
        let pointer = bytes.len() as u32;
        bytes[60..64].copy_from_slice(&pointer.to_be_bytes());
        bytes.extend_from_slice(&chunk(b"rsmp", 0, &map));
    }
    bytes
}

fn sprite_samples(bytes: &[u8]) -> Vec<([u8; 4], u8, u8)> {
    let limits = default_limits();
    let file = iff::decode(bytes, &limits).unwrap();
    let palette = decode_palt(
        &file
            .chunks
            .iter()
            .find(|c| c.key.kind == *b"PALT")
            .unwrap()
            .data,
        &limits,
    )
    .unwrap();
    let sprites = decode_spr2_with_palettes(
        &file
            .chunks
            .iter()
            .find(|c| c.key.kind == *b"SPR2")
            .unwrap()
            .data,
        |_| Some(&palette),
        &limits,
    )
    .unwrap();
    let frame = &sprites.frames[0];
    frame
        .rgba
        .as_ref()
        .unwrap()
        .iter()
        .copied()
        .zip(frame.indices.as_ref().unwrap().iter().copied())
        .zip(frame.depth.as_ref().unwrap().iter().copied())
        .map(|((rgba, index), depth)| (rgba, index, depth))
        .collect()
}

fn resource(bytes: &[u8], kind: &[u8; 4], id: u16) -> iff::IffChunk {
    iff::decode(bytes, &default_limits())
        .unwrap()
        .chunks
        .into_iter()
        .find(|c| c.key == ChunkKey { kind: *kind, id })
        .unwrap()
}

#[test]
fn cli_sprite_roundtrip_and_edits_preserve_unrelated_resources_in_both_layouts() {
    for version in [1000, 1001] {
        let work = Work::new();
        let original = fixture(version, false);
        work.write("source.iff", &original);
        work.call(&["sprite-export", "source.iff", "8", "sprite.json"], true);
        let package = work.json("sprite.json");
        assert_eq!(package["sprite"]["format_version"], version);
        assert_eq!(
            package["sprite"]["frames"][0]["indices_hex"],
            "0102010000000000"
        );
        assert_eq!(
            package["sprite"]["frames"][0]["alpha_hex"],
            "ff7bff0000000000"
        );
        assert_eq!(
            package["sprite"]["frames"][0]["depth_hex"],
            "001309ffffffffff"
        );
        work.call(
            &["sprite-import", "source.iff", "noop.iff", "sprite.json"],
            true,
        );
        assert_eq!(work.read("noop.iff"), original);
        work.call(
            &[
                "sprite-pixel",
                "sprite.json",
                "pixel.json",
                "0",
                "0",
                "0",
                "2",
                "123",
                "17",
            ],
            true,
        );
        work.call(
            &[
                "sprite-palette",
                "pixel.json",
                "edited.json",
                "7",
                "2",
                "40",
                "50",
                "60",
            ],
            true,
        );
        let output = work.call(
            &["sprite-import", "source.iff", "edited.iff", "edited.json"],
            true,
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["quantized_alpha_pixels"], 0);
        let edited = work.read("edited.iff");
        assert_eq!(sprite_samples(&edited)[0], ([40, 50, 60, 123], 2, 17));
        assert_eq!(
            resource(&original, b"ZZZZ", 9),
            resource(&edited, b"ZZZZ", 9)
        );
        let original_sprite = resource(&original, b"SPR2", 8);
        let edited_sprite = resource(&edited, b"SPR2", 8);
        assert_eq!(original_sprite.label, edited_sprite.label);
        assert_eq!(original_sprite.flags, edited_sprite.flags);
        assert_eq!(
            &resource(&edited, b"PALT", 7).data[..16],
            &resource(&original, b"PALT", 7).data[..16]
        );
        work.call(
            &["sprite-import", "edited.iff", "edited.iff", "edited.json"],
            false,
        );
        assert_eq!(work.read("edited.iff"), edited);
    }
}

#[test]
fn indexed_iff_sprite_edit_rebuilds_map_and_late_failures_keep_destination() {
    let work = Work::new();
    let original = fixture(1001, true);
    work.write("source.iff", &original);
    work.write("sentinel.iff", b"existing destination");
    work.call(&["sprite-export", "source.iff", "8", "sprite.json"], true);
    let mut package = work.json("sprite.json");
    package["sprite"]["frames"][0]["width"] = json!(1);
    package["sprite"]["frames"][0]["height"] = json!(1);
    package["sprite"]["frames"][0]["position"] = json!([-32768, 32767]);
    package["sprite"]["frames"][0]["indices_hex"] = json!("02");
    package["sprite"]["frames"][0]["alpha_hex"] = json!("ff");
    package["sprite"]["frames"][0]["depth_hex"] = json!("14");
    work.put_json("edited.json", &package);
    work.call(
        &["sprite-import", "source.iff", "edited.iff", "edited.json"],
        true,
    );
    let edited = work.read("edited.iff");
    let parsed = iff::decode(&edited, &default_limits()).unwrap();
    assert_ne!(original[60..64], edited[60..64]);
    assert_eq!(
        resource(&original, b"ZZZZ", 9),
        resource(&edited, b"ZZZZ", 9)
    );
    // Re-export proves the rebuilt map is a consistent source for a later edit.
    work.call(&["sprite-export", "edited.iff", "8", "again.json"], true);
    work.call(
        &["sprite-import", "edited.iff", "again.iff", "again.json"],
        true,
    );
    assert_eq!(work.read("again.iff"), edited);
    assert_eq!(parsed.chunks.len(), 4);
    package["sprite"]["frames"][0]["alpha_hex"] = json!("7c");
    work.put_json("invalid.json", &package);
    work.call(
        &[
            "sprite-import",
            "source.iff",
            "sentinel.iff",
            "invalid.json",
        ],
        false,
    );
    assert_eq!(work.read("sentinel.iff"), b"existing destination");
    assert_eq!(work.read("source.iff"), original);
}

#[test]
fn palette_only_changes_and_explicit_quantization_preserve_sprite_passthrough() {
    let work = Work::new();
    let original = fixture(1001, false);
    work.write("source.iff", &original);
    work.call(&["sprite-export", "source.iff", "8", "sprite.json"], true);
    work.call(
        &[
            "sprite-palette",
            "sprite.json",
            "palette.json",
            "7",
            "2",
            "40",
            "50",
            "60",
        ],
        true,
    );
    work.call(
        &["sprite-import", "source.iff", "palette.iff", "palette.json"],
        true,
    );
    assert_eq!(
        resource(&original, b"SPR2", 8),
        resource(&work.read("palette.iff"), b"SPR2", 8)
    );
    work.call(
        &[
            "sprite-alpha-mode",
            "sprite.json",
            "source-mode.json",
            "source",
        ],
        true,
    );
    work.call(
        &[
            "sprite-pixel",
            "source-mode.json",
            "alpha.json",
            "0",
            "1",
            "0",
            "2",
            "124",
            "19",
        ],
        true,
    );
    let result = work.call(
        &["sprite-import", "source.iff", "quantized.iff", "alpha.json"],
        true,
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["quantized_alpha_pixels"], 1);
    assert_eq!(
        sprite_samples(&work.read("quantized.iff"))[1],
        ([31, 32, 33, 131], 2, 19)
    );
    work.call(
        &["sprite-export", "quantized.iff", "8", "quantized.json"],
        true,
    );
    work.call(
        &[
            "sprite-alpha-mode",
            "quantized.json",
            "repeat-mode.json",
            "source",
        ],
        true,
    );
    work.call(
        &[
            "sprite-pixel",
            "repeat-mode.json",
            "repeat.json",
            "0",
            "1",
            "0",
            "2",
            "124",
            "19",
        ],
        true,
    );
    let result = work.call(
        &[
            "sprite-import",
            "quantized.iff",
            "repeat.iff",
            "repeat.json",
        ],
        true,
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["quantized_alpha_pixels"],
        1
    );
    assert_eq!(work.read("repeat.iff"), work.read("quantized.iff"));
}

#[test]
fn hand_built_sprite_fixtures_are_valid_source_inputs() {
    for version in [1000, 1001] {
        let original = fixture(version, true);
        assert_eq!(sprite_samples(&original)[1], ([31, 32, 33, 123], 2, 19));
        let doc = ResourceDocument::import(&original, &default_limits()).unwrap();
        assert_eq!(
            sha256(&doc.export(&default_limits()).unwrap()),
            sha256(&original)
        );
    }
}

fn package_json(doc: &ResourceDocument) -> Value {
    serde_json::from_str(
        &doc.export_sprite(8, &default_limits())
            .unwrap()
            .to_json(&default_limits())
            .unwrap(),
    )
    .unwrap()
}
fn package(value: &Value) -> SpritePackage {
    SpritePackage::from_json(&serde_json::to_vec(value).unwrap(), &default_limits()).unwrap()
}

#[test]
fn every_source_guard_and_immutable_identity_conflict_leaves_document_exact() {
    let source = fixture(1001, false);
    let mut doc = ResourceDocument::import(&source, &default_limits()).unwrap();
    let original = package_json(&doc);
    for (name, changed) in [
        ("source hash", {
            let mut p = original.clone();
            p["source_sha256"] = json!("0".repeat(64));
            p
        }),
        ("sprite hash", {
            let mut p = original.clone();
            p["sprite"]["resource_sha256"] = json!("0".repeat(64));
            p
        }),
        ("sprite version", {
            let mut p = original.clone();
            p["sprite"]["format_version"] = json!(1000);
            p
        }),
        ("palette hash", {
            let mut p = original.clone();
            p["palettes"][0]["resource_sha256"] = json!("0".repeat(64));
            p
        }),
        ("palette version", {
            let mut p = original.clone();
            p["palettes"][0]["format_version"] = json!(0);
            p
        }),
        ("palette reserved", {
            let mut p = original.clone();
            p["palettes"][0]["reserved_hex"] = json!("00".repeat(8));
            p
        }),
        ("palette count", {
            let mut p = original.clone();
            let rgb = p["palettes"][0]["colors_rgb_hex"]
                .as_str()
                .unwrap()
                .to_owned();
            p["palettes"][0]["colors_rgb_hex"] = json!(rgb + "010203");
            p
        }),
        ("raw palette identity", {
            let mut p = original.clone();
            p["sprite"]["frames"][0]["raw_palette_id"] = json!(0xa3a3);
            p
        }),
        ("frame count", {
            let mut p = original.clone();
            let mut frame = p["sprite"]["frames"][0].clone();
            frame["index"] = json!(1);
            p["sprite"]["frames"].as_array_mut().unwrap().push(frame);
            p
        }),
    ] {
        let edited = package(&changed);
        assert!(
            doc.import_sprite(&edited, &default_limits()).is_err(),
            "{name}"
        );
        assert_eq!(doc.export(&default_limits()).unwrap(), source, "{name}");
    }
}

#[test]
fn strict_package_schema_rejects_unknown_duplicate_positional_and_malformed_values() {
    let doc = ResourceDocument::import(&fixture(1001, false), &default_limits()).unwrap();
    let source = package_json(&doc);
    let invalid = [
        json!([]),
        {
            let mut p = source.clone();
            p["sprite"] = json!([]);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0] = json!([]);
            p
        },
        {
            let mut p = source.clone();
            p["palettes"][0] = json!([]);
            p
        },
        {
            let mut p = source.clone();
            p["alpha_mode"] = json!({"source": null});
            p
        },
        {
            let mut p = source.clone();
            p["schema_version"] = json!(1.0);
            p
        },
        {
            let mut p = source.clone();
            p["source_sha256"] = json!(null);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["id"] = json!(65536);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["position"] = json!([0, 32768]);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["position"] = json!([0.0, 1]);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["index"] = json!(1);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["flags"] = json!(8);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["palette_id"] = json!(10);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["indices_hex"] = json!("zz".repeat(8));
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["alpha_hex"] = json!("ff");
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["depth_hex"] = json!(null);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]
                .as_object_mut()
                .unwrap()
                .remove("depth_hex");
            p
        },
        {
            let mut p = source.clone();
            p["palettes"] = json!([]);
            p
        },
        {
            let mut p = source.clone();
            let q = p["palettes"][0].clone();
            p["palettes"].as_array_mut().unwrap().push(q);
            p
        },
        {
            let mut p = source.clone();
            p["sprite"]["frames"][0]["typo"] = json!([1, 2, 3]);
            p
        },
    ];
    for (case, value) in invalid.into_iter().enumerate() {
        assert!(
            SpritePackage::from_json(&serde_json::to_vec(&value).unwrap(), &default_limits())
                .is_err(),
            "case {case}"
        );
    }
    let valid = serde_json::to_string(&source).unwrap();
    let duplicate = valid.replacen("{", "{\"schema_version\":1,", 1);
    let error = SpritePackage::from_json(duplicate.as_bytes(), &default_limits()).unwrap_err();
    assert!(error.contains("duplicate field"), "{error}");
    // An unknown name is rejected before parsing a malformed, deep value.
    let unknown = format!("{{\"unknown\":{}", "[".repeat(1000));
    let error = SpritePackage::from_json(unknown.as_bytes(), &default_limits()).unwrap_err();
    assert!(error.contains("unknown field"), "{error}");
    assert!(
        SpritePackage::from_json(format!("{valid} false").as_bytes(), &default_limits()).is_err()
    );
}

#[test]
fn setters_validate_before_mutation_and_channel_changes_require_matching_planes() {
    let mut doc = ResourceDocument::import(&fixture(1001, false), &default_limits()).unwrap();
    let mut edited = doc.export_sprite(8, &default_limits()).unwrap();
    let before = edited.to_json(&default_limits()).unwrap();
    for (point, index, alpha, depth) in [
        ([4, 0], 1, 255, Some(1)),
        ([0, 0], 3, 255, Some(1)),
        ([0, 0], 1, 124, Some(1)),
        ([0, 0], 1, 0, Some(255)),
        ([0, 0], 0, 0, Some(0)),
        ([0, 0], 1, 255, None),
    ] {
        assert!(edited
            .set_pixel(0, point, index, alpha, depth, &default_limits())
            .is_err());
        assert_eq!(edited.to_json(&default_limits()).unwrap(), before);
    }
    assert!(edited.set_palette_color(7, 3, [1, 2, 3]).is_err());
    assert_eq!(edited.to_json(&default_limits()).unwrap(), before);
    edited
        .set_alpha_mode(Spr2AlphaMode::QuantizeLikeSource)
        .unwrap();
    edited
        .set_pixel(0, [0, 0], 1, 124, Some(1), &default_limits())
        .unwrap();
    let source_mode = edited.to_json(&default_limits()).unwrap();
    assert!(edited.set_alpha_mode(Spr2AlphaMode::Exact).is_err());
    assert_eq!(edited.to_json(&default_limits()).unwrap(), source_mode);
    let mut value: Value = serde_json::from_str(&before).unwrap();
    value["sprite"]["frames"][0]["flags"] = json!(5);
    value["sprite"]["frames"][0]["alpha_hex"] = json!("ffffff0000000000");
    value["sprite"]["frames"][0]["depth_hex"] = Value::Null;
    let package = package(&value);
    doc.import_sprite(&package, &default_limits()).unwrap();
    let after = package_json(&doc);
    assert_eq!(after["sprite"]["frames"][0]["flags"], 5);
    assert_eq!(after["sprite"]["frames"][0]["depth_hex"], Value::Null);
}

#[test]
fn source_palette_discovery_uses_only_effective_dependencies_and_keeps_other_palettes_opaque() {
    let limits = default_limits();
    let mut file = iff::decode(&fixture(1001, false), &limits).unwrap();
    let original = file.chunks[1].data.clone();
    let frame = original[12..].to_vec();
    let mut sprite = original[..12].to_vec();
    sprite[4..8].copy_from_slice(&0x12340007u32.to_le_bytes());
    sprite[8..12].copy_from_slice(&3u32.to_le_bytes());
    for raw in [0u16, 0xa3a3, 10] {
        let mut frame = frame.clone();
        frame[16..18].copy_from_slice(&raw.to_le_bytes());
        sprite.extend_from_slice(&frame);
    }
    file.chunks[1].data = sprite;
    let mut second = file.chunks[0].clone();
    second.key.id = 10;
    second.data[19..22].copy_from_slice(&[90, 91, 92]);
    file.chunks.push(second);
    let mut unused = file.chunks[0].clone();
    unused.key.id = 11;
    unused.data = vec![0];
    file.chunks.push(unused.clone());
    let bytes = iff::encode(&file, &limits).unwrap();
    let mut doc = ResourceDocument::import(&bytes, &limits).unwrap();
    let mut package = doc.export_sprite(8, &limits).unwrap();
    let value: Value = serde_json::from_str(&package.to_json(&limits).unwrap()).unwrap();
    assert_eq!(value["palettes"].as_array().unwrap().len(), 2);
    assert_eq!(value["sprite"]["frames"][1]["palette_id"], 7);
    assert_eq!(value["sprite"]["frames"][2]["palette_id"], 10);
    package
        .set_pixel(2, [0, 0], 1, 123, Some(33), &limits)
        .unwrap();
    doc.import_sprite(&package, &limits).unwrap();
    assert_eq!(doc.chunk(unused.key).unwrap(), &unused);
    let colors: std::collections::BTreeMap<_, _> = doc
        .file()
        .chunks
        .iter()
        .filter(|c| c.key.kind == *b"PALT" && c.key.id != 11)
        .map(|c| (c.key.id, decode_palt(&c.data, &limits).unwrap()))
        .collect();
    let set = decode_spr2_with_palettes(
        &doc.chunk(ChunkKey {
            kind: *b"SPR2",
            id: 8,
        })
        .unwrap()
        .data,
        |id| colors.get(&id),
        &limits,
    )
    .unwrap();
    assert_eq!(set.frames[2].rgba.as_ref().unwrap()[0], [90, 91, 92, 123]);
    assert_eq!(set.frames[0].rgba.as_ref().unwrap()[0], [21, 22, 23, 255]);
}

#[test]
fn transparent_u16_index_uses_full_palette_rgb_and_low_byte_stored_index() {
    let limits = default_limits();
    let mut file = iff::decode(&fixture(1001, false), &limits).unwrap();
    let mut palette = decode_palt(&file.chunks[0].data, &limits).unwrap();
    palette.colors.resize(258, [0, 0, 0, 255]);
    palette.colors[257] = [90, 91, 92, 255];
    file.chunks[0].data = encode_palt(&palette, &limits).unwrap();
    // Change the transparent header and every explicit/row-skipped transparent
    // sample is now source index 257 (stored as index byte 1).
    file.chunks[1].data[30..32].copy_from_slice(&257u16.to_le_bytes());
    let bytes = iff::encode(&file, &limits).unwrap();
    let mut doc = ResourceDocument::import(&bytes, &limits).unwrap();
    let mut package = doc.export_sprite(8, &limits).unwrap();
    package
        .set_pixel(0, [0, 0], 1, 0, Some(255), &limits)
        .unwrap();
    doc.import_sprite(&package, &limits).unwrap();
    let output = doc.export(&limits).unwrap();
    assert_eq!(sprite_samples(&output)[0], ([90, 91, 92, 0], 1, 255));
}

#[test]
fn late_sprite_row_failure_does_not_commit_an_earlier_palette_change() {
    let source = fixture(1001, false);
    let mut doc = ResourceDocument::import(&source, &default_limits()).unwrap();
    let mut value = package_json(&doc);
    value["palettes"][0]["colors_rgb_hex"] = json!("0b0c0d2828281f2021");
    let frame = &mut value["sprite"]["frames"][0];
    frame["width"] = json!(9000);
    frame["height"] = json!(1);
    frame["flags"] = json!(1);
    frame["indices_hex"] = json!("01".repeat(9000));
    frame["alpha_hex"] = json!("ff".repeat(9000));
    frame["depth_hex"] = Value::Null;
    let package = package(&value);
    let error = doc.import_sprite(&package, &default_limits()).unwrap_err();
    assert!(error.contains("row exceeds"), "{error}");
    assert_eq!(doc.export(&default_limits()).unwrap(), source);
}

#[test]
fn unsupported_map_allows_exact_sprite_noop_and_rejects_changed_candidate() {
    let mut file = iff::decode(&fixture(1001, true), &default_limits()).unwrap();
    let mut source = fixture(1001, true);
    let map = u32::from_be_bytes(source[60..64].try_into().unwrap()) as usize;
    source[map + 80..map + 84].copy_from_slice(&99u32.to_le_bytes());
    let mut doc = ResourceDocument::import(&source, &default_limits()).unwrap();
    let mut package = doc.export_sprite(8, &default_limits()).unwrap();
    doc.import_sprite(&package, &default_limits()).unwrap();
    assert_eq!(doc.export(&default_limits()).unwrap(), source);
    package
        .set_pixel(0, [0, 0], 2, 255, Some(0), &default_limits())
        .unwrap();
    assert!(doc.import_sprite(&package, &default_limits()).is_err());
    assert_eq!(doc.export(&default_limits()).unwrap(), source);
    // Keep the fixture's unrelated bytes observable too.
    assert_eq!(
        doc.chunk(ChunkKey {
            kind: *b"ZZZZ",
            id: 9
        })
        .unwrap(),
        &file.chunks.remove(2)
    );
}

#[test]
fn package_and_source_limits_cover_aggregate_pixels_and_empty_frame_counts() {
    let source = fixture(1001, false);
    let doc = ResourceDocument::import(&source, &default_limits()).unwrap();
    let value = package_json(&doc);
    let mut low = default_limits();
    low.max_pixels = 7;
    assert!(doc
        .export_sprite(8, &low)
        .unwrap_err()
        .contains("aggregate"));
    assert!(
        SpritePackage::from_json(&serde_json::to_vec(&value).unwrap(), &low)
            .unwrap_err()
            .contains("aggregate")
    );
    let mut two_frames = value;
    let mut second = two_frames["sprite"]["frames"][0].clone();
    second["index"] = json!(1);
    two_frames["sprite"]["frames"]
        .as_array_mut()
        .unwrap()
        .push(second);
    low.max_pixels = 15;
    assert!(
        SpritePackage::from_json(&serde_json::to_vec(&two_frames).unwrap(), &low)
            .unwrap_err()
            .contains("aggregate")
    );
    let mut file = iff::decode(&source, &default_limits()).unwrap();
    let mut sprite = file.chunks[1].data[..12].to_vec();
    sprite[8..12].copy_from_slice(&4u32.to_le_bytes());
    for _ in 0..4 {
        sprite.extend_from_slice(&1001u32.to_le_bytes());
        sprite.extend_from_slice(&18u32.to_le_bytes());
        sprite.extend_from_slice(&0u16.to_le_bytes());
        sprite.extend_from_slice(&65535u16.to_le_bytes());
        sprite.extend_from_slice(&1u32.to_le_bytes());
        sprite.extend_from_slice(&[0; 8]);
        sprite.extend_from_slice(&[0, 0xa0]);
    }
    file.chunks[1].data = sprite;
    let empty = iff::encode(&file, &default_limits()).unwrap();
    let doc = ResourceDocument::import(&empty, &default_limits()).unwrap();
    low.max_pixels = 0;
    low.max_frames = 3;
    assert!(doc
        .export_sprite(8, &low)
        .unwrap_err()
        .contains("frame count"));
    low.max_frames = 4;
    let package = doc.export_sprite(8, &low).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&package.to_json(&low).unwrap()).unwrap()["sprite"]["frames"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    low.max_total_decoded_bytes = 512;
    assert!(doc.export_sprite(8, &low).is_err());
}

#[test]
fn tighter_caller_input_limit_is_enforced_on_existing_document() {
    let source = fixture(1001, false);
    let mut doc = ResourceDocument::import(&source, &default_limits()).unwrap();
    let package = doc.export_sprite(8, &default_limits()).unwrap();
    let mut low = default_limits();
    low.max_input_bytes = source.len() - 1;
    assert!(doc.export_sprite(8, &low).is_err());
    assert!(doc.import_sprite(&package, &low).is_err());
    assert_eq!(doc.export(&default_limits()).unwrap(), source);
}

#[test]
fn sprite_publication_accounts_for_retained_document_and_iff_writer_workspace() {
    let limits = default_limits();
    let mut source = fixture(1001, false);
    // Tiny payloads still require IFF collection storage. A final publication
    // must admit the writer alongside the retained document/package/planes.
    for id in 0..1000 {
        source.extend_from_slice(&chunk(b"TEST", id, &[]));
    }
    let mut doc = ResourceDocument::import(&source, &limits).unwrap();
    let mut package = doc.export_sprite(8, &limits).unwrap();
    package
        .set_pixel(0, [0, 0], 2, 255, Some(0), &limits)
        .unwrap();
    let mut low = limits;
    low.max_total_decoded_bytes = 2_400_000;
    assert!(doc.import_sprite(&package, &low).is_err());
    assert_eq!(doc.export(&limits).unwrap(), source);
    doc.import_sprite(&package, &limits).unwrap();
    assert_eq!(
        sprite_samples(&doc.export(&limits).unwrap())[0],
        ([31, 32, 33, 255], 2, 0)
    );
}

#[test]
fn typed_import_refuses_source_pixels_that_canonical_encoding_would_normalize() {
    let limits = default_limits();
    let source = fixture(1001, false);
    let original_doc = ResourceDocument::import(&source, &limits).unwrap();
    let mut value = package_json(&original_doc);
    let mut file = iff::decode(&source, &limits).unwrap();
    let mut sprite = file.chunks[1].data[..36].to_vec();
    sprite[16..20].copy_from_slice(&24u32.to_le_bytes());
    sprite[22..24].copy_from_slice(&1u16.to_le_bytes());
    // A short source row leaves its remaining RGB bytes at zero. Those hidden
    // RGB bytes differ from PALT's transparent RGB (11,12,13), so the typed
    // writer cannot retain them even though the legacy reader accepts them.
    sprite.extend_from_slice(&[6, 0, 1, 0xc0, 1, 0, 0, 0xa0]);
    file.chunks[1].data = sprite;
    let bytes = iff::encode(&file, &limits).unwrap();
    value["source_sha256"] = json!(sha256(&bytes));
    value["sprite"]["resource_sha256"] = json!(sha256(&file.chunks[1].data));
    value["sprite"]["frames"][0]["height"] = json!(1);
    value["sprite"]["frames"][0]["indices_hex"] = json!("01000000");
    value["sprite"]["frames"][0]["alpha_hex"] = json!("ff000000");
    value["sprite"]["frames"][0]["depth_hex"] = json!("00ffffff");
    let package = package(&value);
    let mut doc = ResourceDocument::import(&bytes, &limits).unwrap();
    assert_eq!(sprite_samples(&bytes)[1], ([0, 0, 0, 0], 0, 255));
    assert!(doc.export_sprite(8, &limits).is_err());
    assert!(doc.import_sprite(&package, &limits).is_err());
    assert_eq!(doc.export(&limits).unwrap(), bytes);
}

#[test]
fn sprite_cli_rejects_unsafe_paths_and_oversized_package_before_publication() {
    let work = Work::new();
    work.write("source.iff", fixture(1001, false));
    work.write("sentinel.json", b"existing package output");
    work.call(&["sprite-export", "source.iff", "8", "sprite.json"], true);
    work.call(
        &[
            "sprite-alpha-mode",
            "../sprite.json",
            "sentinel.json",
            "source",
        ],
        false,
    );
    let file = fs::File::create(work.0.join("huge.json")).unwrap();
    file.set_len(MAX_SPRITE_PACKAGE_BYTES as u64 + 1).unwrap();
    work.call(
        &["sprite-alpha-mode", "huge.json", "sentinel.json", "source"],
        false,
    );
    assert_eq!(work.read("sentinel.json"), b"existing package output");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("source.iff", work.0.join("link.iff")).unwrap();
        work.call(&["sprite-export", "link.iff", "8", "sentinel.json"], false);
        std::os::unix::fs::symlink("sentinel.json", work.0.join("out.json")).unwrap();
        work.call(
            &["sprite-alpha-mode", "sprite.json", "out.json", "source"],
            false,
        );
        assert_eq!(work.read("sentinel.json"), b"existing package output");
    }
}

#[test]
fn authored_sprite_and_palette_changes_recook_and_invalidate_dependent_cache_keys() {
    use wonderland_asset_cooker::{
        interchange::import_spec,
        manifest::{cook_resources, CookLimits},
        registry::CookSpec,
    };
    use wonderland_content_ir::manifest::LoadPhase;
    let limits = default_limits();
    let work = Work::new();
    let mut file = iff::decode(&fixture(1001, false), &limits).unwrap();
    let mut drawing = file.chunks[0].clone();
    drawing.key = ChunkKey {
        kind: *b"DGRP",
        id: 12,
    };
    drawing.data = 20000u16.to_le_bytes().to_vec();
    drawing.data.extend_from_slice(&[1, 0, 1, 0, 1, 1]);
    for value in [0u16, 8, 0, 0, 0, 0] {
        drawing.data.extend_from_slice(&value.to_le_bytes());
    }
    file.chunks.push(drawing);
    let original = iff::encode(&file, &limits).unwrap();
    work.write("source.iff", &original);
    work.write("control.iff", &original);
    let cook_limits = CookLimits::default();
    let mut spec = CookSpec::from_json(
        include_bytes!("../../fixtures/packs/demo.json"),
        &cook_limits,
    )
    .unwrap();
    spec.sources[0].path = "source.iff".into();
    spec.overrides.clear();
    let mut control = spec.sources[0].clone();
    control.id = "control".into();
    control.path = "control.iff".into();
    control.source_name = Some("Control.iff".into());
    control.pack_group = "control".into();
    spec.sources.push(control);
    let cook = || {
        let imported = import_spec(&spec, &work.0, &cook_limits).unwrap();
        cook_resources(
            &spec.source_baseline,
            spec.tuning_version.clone(),
            &imported.resources,
            &cook_limits,
        )
        .unwrap()
    };
    let baseline = cook();
    let repeated = cook();
    assert_eq!(
        baseline
            .manifest
            .canonical_bytes(&cook_limits.manifest)
            .unwrap(),
        repeated
            .manifest
            .canonical_bytes(&cook_limits.manifest)
            .unwrap()
    );
    assert_eq!(baseline.packs, repeated.packs);
    let sprite_id = "fixture/chunk-53505232-0008";
    let drawing_id = "fixture/chunk-44475250-000c";
    let palette_id = "fixture/chunk-50414c54-0007";
    let opaque_id = "fixture/chunk-5a5a5a5a-0009";
    let control_id = "control/chunk-53505232-0008";
    for palette_only in [false, true] {
        let mut doc = ResourceDocument::import(&original, &limits).unwrap();
        let mut package = doc.export_sprite(8, &limits).unwrap();
        if palette_only {
            package.set_palette_color(7, 2, [40, 50, 60]).unwrap();
        } else {
            package
                .set_pixel(0, [0, 0], 2, 123, Some(17), &limits)
                .unwrap();
        }
        doc.import_sprite(&package, &limits).unwrap();
        work.write("source.iff", doc.export(&limits).unwrap());
        let changed = cook();
        assert_eq!(
            baseline.manifest.resources[drawing_id].content_hash,
            changed.manifest.resources[drawing_id].content_hash
        );
        assert_eq!(
            baseline.manifest.resources[opaque_id].content_hash,
            changed.manifest.resources[opaque_id].content_hash
        );
        let unchanged_id = if palette_only { sprite_id } else { palette_id };
        assert_eq!(
            baseline.manifest.resources[unchanged_id].content_hash,
            changed.manifest.resources[unchanged_id].content_hash
        );
        for id in [sprite_id, drawing_id] {
            assert_ne!(
                baseline
                    .manifest
                    .derived_cache_key(id, "sprite-authoring-v1", "full2d", &cook_limits.manifest)
                    .unwrap(),
                changed
                    .manifest
                    .derived_cache_key(id, "sprite-authoring-v1", "full2d", &cook_limits.manifest)
                    .unwrap()
            );
        }
        assert_eq!(
            baseline
                .manifest
                .derived_cache_key(
                    control_id,
                    "sprite-authoring-v1",
                    "full2d",
                    &cook_limits.manifest
                )
                .unwrap(),
            changed
                .manifest
                .derived_cache_key(
                    control_id,
                    "sprite-authoring-v1",
                    "full2d",
                    &cook_limits.manifest
                )
                .unwrap()
        );
        let closure = changed
            .manifest
            .load_plan(
                &[drawing_id.into()],
                LoadPhase::All,
                None,
                &Default::default(),
                &cook_limits.manifest,
            )
            .unwrap();
        assert_eq!(closure.resources.len(), 3);
    }
}
