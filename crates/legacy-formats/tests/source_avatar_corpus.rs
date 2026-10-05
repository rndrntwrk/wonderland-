// SPDX-License-Identifier: MPL-2.0
//! Read-only source census. Original assets are not copied into generated packs.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
use wonderland_legacy_formats::{reconstruction, vitaboy, Limits};

fn files(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            files(&entry.path(), output);
        } else if kind.is_file() {
            output.push(entry.path());
        }
    }
}

#[test]
fn checked_in_original_avatar_and_reconstruction_payloads_roundtrip_exactly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    files(
        &root.join("TSOClient/FSO.Content.TSO/Content/Avatar"),
        &mut paths,
    );
    files(
        &root.join("TSOClient/tso.content/Content/MeshReplace"),
        &mut paths,
    );
    paths.sort();
    let limits = Limits::default();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    for path in paths {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(ext.as_str(), "anim" | "mesh" | "apr" | "bnd" | "fsom") {
            continue;
        }
        assert!(path.metadata().unwrap().len() <= limits.max_resource_bytes as u64);
        let bytes = std::fs::read(&path).unwrap();
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        *counts.entry(ext.clone()).or_default() += 1;
        macro_rules! exact {
            ($decode:path, $encode:path) => {{
                $decode(&bytes, &limits)
                    .and_then(|value| $encode(&value, &limits))
                    .map(|encoded| encoded == bytes)
                    .map_err(|e| e.to_string())
            }};
        }
        let outcome = match ext.as_str() {
            "anim" => exact!(vitaboy::decode_animation, vitaboy::encode_animation),
            "mesh" => exact!(vitaboy::decode_mesh, vitaboy::encode_mesh),
            "apr" => exact!(vitaboy::decode_appearance, vitaboy::encode_appearance),
            "bnd" => exact!(vitaboy::decode_binding, vitaboy::encode_binding),
            "fsom" => {
                // Independently unwrap the original gzip, then compare every
                // raw source field. Compressor byte choices are not the claim.
                let mut payload = Vec::new();
                flate2::read::GzDecoder::new(bytes.as_slice())
                    .take(limits.max_resource_bytes as u64 + 1)
                    .read_to_end(&mut payload)
                    .unwrap();
                assert!(payload.len() <= limits.max_resource_bytes);
                reconstruction::decode_fsom(&bytes, &limits)
                    .and_then(|value| reconstruction::encode_fsom_payload(&value, &limits))
                    .map(|encoded| encoded == payload)
                    .map_err(|e| e.to_string())
            }
            _ => unreachable!(),
        };
        let hash = format!("{:x}", Sha256::digest(&bytes));
        match outcome {
            Ok(true) => println!("asset\t{relative}\t{hash}\t{}\texact", bytes.len()),
            other => {
                println!("asset\t{relative}\t{hash}\t{}\t{other:?}", bytes.len());
                failures.push(format!("{relative}: {other:?}"));
            }
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("anim".into(), 50),
            ("apr".into(), 120),
            ("bnd".into(), 120),
            ("fsom".into(), 44),
            ("mesh".into(), 121),
        ])
    );
    println!("original_source\t4c6b3e8f5835b228723caea3c9f683c62f244f73\t{counts:?}");
    assert!(
        failures.is_empty(),
        "source payload failures:\n{}",
        failures.join("\n")
    );
}
