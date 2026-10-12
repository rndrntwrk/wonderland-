use crate::Result;
use bincode::Options;
use serde::Deserialize;
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    content_bridge::cooked::{CookedLoadLimits, PackBytes, PreparedRelease, SOURCE_BASELINE},
};
use wonderland_native_host_contracts::{DecimalId, HostBindingV1};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub release_dir: String,
    pub binding_sha256: String,
    pub lot_file: String,
    pub lot_sha256: String,
    pub mode: VmMode,
    pub lot_id: DecimalId,
    pub authority_epoch: DecimalId,
    pub lot_incarnation: DecimalId,
    pub seed: DecimalId,
    pub utc_start_dotnet_ticks: DecimalId,
    pub effect_namespace: DecimalId,
}
pub fn read(path: &Path, cap: usize) -> Result<Vec<u8>> {
    // Deployment owns these immutable files. Reject observed symlinks/traversal.
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir | Component::Prefix(_) => return Err("invalid input path".into()),
            Component::CurDir => {}
            _ => {
                current.push(component.as_os_str());
                if std::fs::symlink_metadata(&current)?
                    .file_type()
                    .is_symlink()
                {
                    return Err("symlink input refused".into());
                }
            }
        }
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > cap as u64 {
        return Err("input file size/type".into());
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > cap {
        return Err("input file exceeded bound".into());
    }
    Ok(bytes)
}
pub fn load(path: &Path) -> Result<(GameRuntime, RuntimeConfig, HostBindingV1, serde_json::Value)> {
    let config: Config = serde_json::from_slice(&read(path, 65536)?)?;
    if config.schema_version != 1
        || config.lot_id.0 == 0
        || config.authority_epoch.0 == 0
        || config.lot_incarnation.0 == 0
        || config.effect_namespace.0 == 0
    {
        return Err("invalid host identity".into());
    }
    let root = Path::new(&config.release_dir);
    let limits = CookedLoadLimits::default();
    let binding = read(&root.join("binding.json"), limits.max_binding_bytes)?;
    let manifest = read(
        &root.join("manifest.json"),
        limits.manifest.max_manifest_bytes,
    )?;
    let expected = Digest::try_from(config.binding_sha256.clone()).map_err(|e| e.to_string())?;
    let prepared = PreparedRelease::from_binding(&binding, &expected, &manifest, &limits)?;
    let total = prepared.required_packs().iter().try_fold(0u64, |n, p| {
        n.checked_add(p.byte_len).ok_or("pack overflow")
    })?;
    if total > limits.max_total_pack_bytes {
        return Err("pack total limit".into());
    }
    let mut payloads = Vec::new();
    for p in prepared.required_packs() {
        let bytes = read(
            &root.join(format!("{}.wlp", p.digest.as_str())),
            usize::try_from(p.byte_len)?,
        )?;
        if bytes.len() as u64 != p.byte_len {
            return Err("pack length differs".into());
        }
        payloads.push(bytes);
    }
    let refs: Vec<_> = prepared
        .required_packs()
        .iter()
        .zip(&payloads)
        .map(|(p, bytes)| PackBytes {
            digest: &p.digest,
            bytes,
        })
        .collect();
    let loaded = prepared.load(&refs, &limits)?;
    let report = serde_json::to_value(&loaded.report)?;
    let lot_bytes = read(Path::new(&config.lot_file), 16 * 1024 * 1024)?;
    if Digest::of(&lot_bytes).as_str() != config.lot_sha256 {
        return Err("lot digest mismatch".into());
    }
    let lot: LotModel = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(16 * 1024 * 1024)
        .reject_trailing_bytes()
        .deserialize(&lot_bytes)?;
    let mut runtime_config = RuntimeConfig::new(
        config.mode,
        config.lot_id.0,
        config.authority_epoch.0,
        config.seed.0,
    );
    runtime_config.effect_namespace = config.effect_namespace.0;
    runtime_config.utc_start_dotnet_ticks = i64::try_from(config.utc_start_dotnet_ticks.0)?;
    let runtime =
        GameRuntime::from_cooked(loaded, lot, runtime_config.clone(), RuntimeRole::Authority)?;
    let identity = HostBindingV1 {
        protocol: 1,
        source_pin: SOURCE_BASELINE.into(),
        binding_sha256: config.binding_sha256,
        lot_incarnation: config.lot_incarnation,
    };
    Ok((runtime, runtime_config, identity, report))
}
