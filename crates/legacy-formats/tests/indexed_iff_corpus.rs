// SPDX-License-Identifier: MPL-2.0
//! Opt-in, bounded checks of originals already present in the source checkout.
//! These tests do not redistribute original assets or run without their input.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use wonderland_legacy_formats::iff::{ChunkKey, IffChunk, IffDocument};
use wonderland_legacy_formats::{reader::Reader, ErrorKind, Limits};

fn limits() -> Limits {
    Limits {
        max_input_bytes: 16 * 1024 * 1024,
        max_resource_bytes: 16 * 1024 * 1024,
        max_total_decoded_bytes: 32 * 1024 * 1024,
        max_entries: 10_000,
        max_string_bytes: 64,
        ..Limits::default()
    }
}

fn objects() -> PathBuf {
    std::env::var_os("WONDERLAND_SOURCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .join("TSOClient/FSO.Content.TSO/Content/Objects")
}

fn bounded_read(path: &Path) -> Vec<u8> {
    assert!(!path.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(path.metadata().unwrap().len() <= limits().max_input_bytes as u64);
    let bytes = std::fs::read(path).unwrap();
    limits().check_input(&bytes).unwrap();
    bytes
}

// A separate byte-level verifier checks emitted map entries against actual
// envelope starts. It uses no production map reader or writer helper.
fn verify_map(bytes: &[u8]) -> (u32, usize) {
    let mut envelopes = BTreeMap::new();
    let pointer = u32::from_be_bytes(bytes[60..64].try_into().unwrap()) as usize;
    let mut at = 64;
    let mut map = None;
    while at < bytes.len() {
        let size = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        assert!(size >= 76 && at + size <= bytes.len());
        if at == pointer {
            assert_eq!(&bytes[at..at + 4], b"rsmp");
            map = Some(&bytes[at + 76..at + size]);
        } else {
            let label = &bytes[at + 12..at + 76];
            let length = label.iter().position(|&b| b == 0).unwrap_or(64);
            envelopes.insert(
                at,
                (
                    bytes[at..at + 4].to_vec(),
                    u16::from_be_bytes(bytes[at + 8..at + 10].try_into().unwrap()),
                    u16::from_be_bytes(bytes[at + 10..at + 12].try_into().unwrap()),
                    label[..length].to_vec(),
                ),
            );
        }
        at += size;
    }
    let map = map.unwrap();
    let mut reader = Reader::new(map);
    assert_eq!(reader.u32_le().unwrap(), 0);
    let version = reader.u32_le().unwrap();
    assert!(version <= 1);
    assert_eq!(reader.read_bytes(4).unwrap(), b"pmsr");
    let size = reader.u32_le().unwrap() as usize;
    assert!(size == 0 || size == map.len() + 76 || size == map.len() - 12);
    let count = reader.u32_le().unwrap();
    let mut checked = 0;
    for _ in 0..count {
        let mut kind = reader.read_bytes(4).unwrap().to_vec();
        kind.reverse();
        let count = reader.u32_le().unwrap();
        for _ in 0..count {
            let offset = reader.u32_le().unwrap() as usize;
            let id = reader.u16_le().unwrap();
            if version == 1 {
                assert_eq!(reader.u16_le().unwrap(), 0);
            }
            let flags = reader.u16_le().unwrap();
            let label = if version == 0 {
                let mut name = Vec::new();
                loop {
                    let byte = reader.u8().unwrap();
                    if byte == 0 {
                        break;
                    }
                    name.push(byte);
                    assert!(name.len() <= 64);
                }
                if !reader.position().is_multiple_of(2) {
                    reader.skip(1).unwrap();
                }
                name
            } else {
                let length = reader.u8().unwrap() as usize;
                assert!(length <= 64);
                reader.read_bytes(length).unwrap().to_vec()
            };
            assert_eq!(
                envelopes.remove(&offset),
                Some((kind.clone(), id, flags, label))
            );
            checked += 1;
        }
    }
    assert_eq!(reader.remaining(), 0);
    assert!(envelopes.is_empty());
    (version, checked)
}

fn save_if_requested(name: &str, edit: usize, bytes: &[u8]) {
    if let Some(directory) = std::env::var_os("WONDERLAND_IFF_EDIT_OUTPUT") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{name}.edit-{edit}.iff")), bytes).unwrap();
    }
}

fn apply_edit(document: &mut IffDocument, edit: usize) {
    let resource = document
        .file()
        .chunks
        .iter()
        .position(|c| c.key.kind != *b"rsmp" && !c.data.is_empty())
        .unwrap();
    match edit {
        0 => document.file_mut().chunks[resource].data[0] ^= 1,
        1 => document.file_mut().chunks[resource]
            .data
            .extend_from_slice(&[0x41, 0x42, 0x43]),
        2 => document.file_mut().chunks.insert(
            0,
            IffChunk {
                key: ChunkKey {
                    kind: *b"wTST",
                    id: 0x1234,
                },
                flags: 0,
                label: [0; 64],
                data: vec![1, 2],
            },
        ),
        3 => {
            document.file_mut().chunks.remove(resource);
        }
        4 => document.file_mut().chunks.reverse(),
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "requires the original source asset checkout; run explicitly with --ignored"]
fn original_indexed_corpus_has_exact_passthrough_and_bounded_edit_dispositions() {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(objects()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("iff") {
            files.push(path);
        }
        assert!(files.len() <= 512);
    }
    files.sort();
    let mut read_bytes = 0usize;
    let (mut indexed, mut exact, mut duplicates, mut unsupported, mut edited, mut entries) =
        (0, 0, 0, 0, 0, 0);
    let mut size_styles = [0usize; 3];
    for path in files {
        let name = path.file_name().unwrap().to_str().unwrap();
        let bytes = bounded_read(&path);
        read_bytes += bytes.len();
        assert!(read_bytes <= 512 * 1024 * 1024);
        if bytes[60..64] == [0; 4] {
            continue;
        }
        indexed += 1;
        let original = match IffDocument::decode(&bytes, &limits()) {
            Ok(value) => value,
            Err(error) => {
                assert!(
                    matches!(
                        name,
                        "stp_wfloveseat.iff" | "stp_wfchair.iff" | "stp_wfplant.iff"
                    ),
                    "unexpected strict rejection {name}: {error}"
                );
                assert_eq!(error.kind, ErrorKind::Duplicate);
                duplicates += 1;
                continue;
            }
        };
        assert_eq!(
            original.encode(&limits()).unwrap(),
            bytes,
            "passthrough {name}"
        );
        exact += 1;
        let excluded = matches!(
            name,
            "k8vteplantts.iff" | "k8tqbbqts.iff" | "k8capmirrorsv.iff"
        );
        for edit in 0..5 {
            let mut document = original.clone();
            apply_edit(&mut document, edit);
            let result = document.encode(&limits());
            if excluded {
                let error = result.unwrap_err();
                assert_eq!(
                    error.kind,
                    if name == "k8vteplantts.iff" {
                        ErrorKind::UnsupportedVersion
                    } else {
                        ErrorKind::InvalidData
                    }
                );
                if edit == 0 {
                    unsupported += 1;
                }
                continue;
            }
            let result =
                result.unwrap_or_else(|error| panic!("edit {edit} failed for {name}: {error}"));
            assert_eq!(
                verify_map(&result).0,
                0,
                "no original version-1 corpus evidence exists"
            );
            let reopened = IffDocument::decode(&result, &limits()).unwrap();
            let expected: Vec<_> = document
                .file()
                .chunks
                .iter()
                .filter(|c| c.key.kind != *b"rsmp")
                .collect();
            let actual: Vec<_> = reopened
                .file()
                .chunks
                .iter()
                .filter(|c| c.key.kind != *b"rsmp")
                .collect();
            assert_eq!(
                actual, expected,
                "unrelated chunk changed for {name}, edit {edit}"
            );
            assert_eq!(reopened.encode(&limits()).unwrap(), result);
            save_if_requested(name, edit, &result);
            edited += 1;
        }
        if !excluded {
            entries += verify_map(&bytes).1;
            let body = &original
                .file()
                .chunks
                .iter()
                .find(|c| c.key.kind == *b"rsmp")
                .unwrap()
                .data;
            let field = u32::from_le_bytes(body[12..16].try_into().unwrap()) as usize;
            size_styles[if field == 0 {
                0
            } else if field == body.len() + 76 {
                1
            } else {
                2
            }] += 1;
        }
    }
    assert_eq!(
        (indexed, exact, duplicates, unsupported, edited, entries),
        (198, 195, 3, 3, 960, 13_084)
    );
    assert_eq!(size_styles, [10, 88, 94]);
    println!("indexed={indexed}, exact={exact}, strict_duplicates={duplicates}, unsupported_edits={unsupported}, successful_edits={edited}, map_entries={entries}, size_styles={size_styles:?}");
}

#[test]
#[ignore = "requires source-derived version-1 input path in WONDERLAND_IFF_V1_SOURCE"]
fn source_derived_v1_input_supports_all_five_structural_edits() {
    let path = PathBuf::from(
        std::env::var_os("WONDERLAND_IFF_V1_SOURCE")
            .expect("set path to independently prepared source-derived version-1 IFF"),
    );
    let bytes = bounded_read(&path);
    assert_eq!(verify_map(&bytes), (1, 39));
    let original = IffDocument::decode(&bytes, &limits()).unwrap();
    assert_eq!(original.encode(&limits()).unwrap(), bytes);
    for edit in 0..5 {
        let mut document = original.clone();
        apply_edit(&mut document, edit);
        let result = document.encode(&limits()).unwrap();
        assert_eq!(verify_map(&result).0, 1);
        save_if_requested("source-derived-v1", edit, &result);
    }
}
