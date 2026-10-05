// SPDX-License-Identifier: MPL-2.0
//! Bounded filesystem operations for a local operator, without archive-derived paths.
use crate::{
    manifest::{CookLimits, CookedContent},
    packs::verify_pack,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use wonderland_content_ir::manifest::{AssetManifest, Digest, ManifestError, ManifestResult};
fn err(e: impl std::fmt::Display) -> ManifestError {
    ManifestError(e.to_string())
}
pub fn validate_relative(value: &str) -> ManifestResult<()> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || value
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(err("unsafe relative import path"));
    }
    Ok(())
}
/// Refuse symlink components and nonregular sources before opening/allocation.
pub fn read_bounded(path: &Path, max: usize) -> ManifestResult<Vec<u8>> {
    reject_symlinks(path)?;
    let leaf = fs::symlink_metadata(path).map_err(err)?;
    if !leaf.is_file() || leaf.len() > max as u64 {
        return Err(err("source is nonregular or exceeds byte limit"));
    }
    let read_cap = max
        .checked_add(1)
        .ok_or_else(|| err("read limit overflow"))?;
    let mut file = File::open(path).map_err(err)?;
    let metadata = file.metadata().map_err(err)?;
    if !metadata.is_file() || metadata.len() > max as u64 {
        return Err(err("source is nonregular or exceeds byte limit"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(read_cap as u64)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > max {
        return Err(err("file grew beyond byte limit"));
    }
    Ok(bytes)
}
pub fn reject_symlinks(path: &Path) -> ManifestResult<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(err)?.join(path)
    };
    let mut cursor = PathBuf::new();
    for component in absolute.components() {
        cursor.push(component.as_os_str());
        let m = fs::symlink_metadata(&cursor).map_err(err)?;
        if m.file_type().is_symlink() {
            return Err(err("symlink paths are forbidden"));
        }
    }
    Ok(())
}
pub fn read_import(root: &Path, relative: &str, max: usize) -> ManifestResult<Vec<u8>> {
    validate_relative(relative)?;
    read_bounded(&root.join(relative), max)
}
fn write_new(path: &Path, bytes: &[u8]) -> ManifestResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.sync_all().map_err(err)
}
/// All construction and integrity checks finish before creating output. Manifest is committed last.
pub fn write_release(
    output: &Path,
    content: &CookedContent,
    report: &[u8],
    limits: &CookLimits,
) -> ManifestResult<Digest> {
    let manifest = content.manifest.canonical_bytes(&limits.manifest)?;
    if !content.packs.keys().eq(content.manifest.packs.keys()) {
        return Err(err(
            "supplied packs do not exactly cover manifest pack identities",
        ));
    }
    if report.len() > limits.manifest.max_manifest_bytes {
        return Err(err("import report exceeds byte limit"));
    }
    for (hash, bytes) in &content.packs {
        verify_pack(bytes, hash, &limits.pack)?
            .validate_manifest(&content.manifest, &limits.manifest)?;
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    reject_symlinks(parent)?;
    fs::create_dir(output).map_err(err)?; // atomic no-clobber claim of the new release directory
    let result = (|| {
        for (hash, bytes) in &content.packs {
            write_new(&output.join(format!("{}.wlp", hash.as_str())), bytes)?;
        }
        write_new(&output.join("import-report.json"), report)?;
        write_new(&output.join("manifest.json"), &manifest)?;
        File::open(output).map_err(err)?.sync_all().map_err(err)?;
        Ok(Digest::of(&manifest))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}
pub fn verify_release(
    output: &Path,
    expected: Option<&Digest>,
    limits: &CookLimits,
) -> ManifestResult<(AssetManifest, Digest)> {
    let bytes = read_bounded(
        &output.join("manifest.json"),
        limits.manifest.max_manifest_bytes,
    )?;
    let manifest = if let Some(hash) = expected {
        AssetManifest::from_json_verified(&bytes, hash, &limits.manifest)?
    } else {
        AssetManifest::from_json(&bytes, &limits.manifest)?
    };
    let mut total = 0u64;
    for (hash, record) in &manifest.packs {
        total = total
            .checked_add(record.byte_len)
            .ok_or_else(|| err("release byte count overflow"))?;
        if total > limits.manifest.max_download_bytes {
            return Err(err("release byte budget exceeded"));
        }
        let pack = read_bounded(
            &output.join(format!("{}.wlp", hash.as_str())),
            limits.pack.max_pack_bytes,
        )?;
        if pack.len() as u64 != record.byte_len {
            return Err(err("pack byte count differs from manifest"));
        }
        let index = verify_pack(&pack, hash, &limits.pack)?;
        index.validate_manifest(&manifest, &limits.manifest)?;
        for id in index.entries().keys() {
            let record = &manifest.resources[id];
            crate::manifest::validate_payload(
                &crate::manifest::PreparedResource {
                    id: id.clone(),
                    group: "verify".into(),
                    kind: record.kind,
                    codec: record.codec,
                    payload: index.get(id)?.to_vec(),
                    dependencies: record.dependencies.clone(),
                    simulation_critical: record.simulation_critical,
                    locale: record.locale.clone(),
                    variants: record.variants.clone(),
                    provenance: record.provenance.clone(),
                },
                &limits.legacy,
            )?;
        }
    }
    Ok((manifest, Digest::of(&bytes)))
}
