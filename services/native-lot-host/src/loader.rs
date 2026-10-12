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
use wonderland_native_host_contracts::{
    CursorV1, DecimalId, HostBindingV1, RecoveryGrantV1, TakeoverMarkerV1,
};

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
    #[serde(default)]
    pub recovery: Option<Recovery>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    pub checkpoint_file: String,
    pub checkpoint_sha256: String,
    #[serde(default)]
    pub tail_file: Option<String>,
    #[serde(default)]
    pub tail_sha256: Option<String>,
    #[serde(default)]
    pub additional_tail_files: Vec<PinnedTail>,
    pub expected_cursor: CursorV1,
    #[serde(default)]
    pub takeover: Option<RecoveryGrantV1>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedTail {
    pub file: String,
    pub sha256: String,
}
fn pinned(path: &str, expected: &str, cap: usize) -> Result<Vec<u8>> {
    let bytes = read(Path::new(path), cap)?;
    if Digest::of(&bytes).as_str() != expected {
        return Err("recovery input digest mismatch".into());
    }
    Ok(bytes)
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
pub type LoadedHost = (
    GameRuntime,
    RuntimeConfig,
    HostBindingV1,
    serde_json::Value,
    Option<TakeoverMarkerV1>,
);
pub fn load(path: &Path) -> Result<LoadedHost> {
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
    let mut runtime =
        GameRuntime::from_cooked(loaded, lot, runtime_config.clone(), RuntimeRole::Authority)?;
    let mut identity = HostBindingV1 {
        protocol: 1,
        source_pin: SOURCE_BASELINE.into(),
        binding_sha256: config.binding_sha256.clone(),
        lot_incarnation: config.lot_incarnation,
    };
    let mut takeover_marker = None;
    if let Some(recovery) = config.recovery {
        let checkpoint = pinned(
            &recovery.checkpoint_file,
            &recovery.checkpoint_sha256,
            8 * 1024 * 1024,
        )?;
        runtime.restore(&checkpoint)?;
        if recovery.additional_tail_files.len() > 64 {
            return Err("recovery tail file count limit".into());
        }
        let mut tail_bytes = 0usize;
        let mut frame_count = 0usize;
        let mut tails = match (recovery.tail_file, recovery.tail_sha256) {
            (Some(file), Some(sha256)) => vec![PinnedTail { file, sha256 }],
            (None, None) => Vec::new(),
            _ => return Err("recovery tail path and digest must be paired".into()),
        };
        tails.extend(recovery.additional_tail_files);
        if tails.len() > 64 {
            return Err("recovery tail file count limit".into());
        }
        for part in tails {
            let tail = pinned(&part.file, &part.sha256, 8 * 1024 * 1024 - tail_bytes)?;
            tail_bytes += tail.len();
            match wonderland_game_runtime::live_wire::decode_packet(&tail, Default::default())? {
                wonderland_game_runtime::live_wire::Packet::Ticks(frames) => {
                    frame_count += frames.len();
                    if frame_count > 64 {
                        return Err("recovery tail count limit".into());
                    }
                    for frame in frames {
                        let result = runtime.apply_accepted(&frame.accepted)?;
                        if result.state_hash != frame.state_hash {
                            return Err("recovery accepted state hash mismatch".into());
                        }
                    }
                }
                _ => return Err("recovery requires accepted tick tail".into()),
            }
        }
        let state = runtime.sim().state();
        let actual = CursorV1 {
            lot_id: DecimalId(state.lot_id),
            authority_epoch: DecimalId(state.authority_epoch),
            completed_tick: DecimalId(state.completed_tick),
            state_hash: runtime
                .sim()
                .state_hash()?
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        };
        if actual != recovery.expected_cursor {
            return Err("recovery did not reach committed cursor".into());
        }
        if let Some(grant) = recovery.takeover {
            let mut next_binding = identity.clone();
            next_binding.lot_incarnation = DecimalId(
                identity
                    .lot_incarnation
                    .0
                    .checked_add(1)
                    .ok_or("incarnation exhausted")?,
            );
            let next_epoch = actual
                .authority_epoch
                .0
                .checked_add(1)
                .ok_or("authority epoch exhausted")?;
            if grant.schema_version != 1
                || grant.old_binding != identity
                || grant.old_cursor != actual
                || grant.new_binding != next_binding
                || grant.new_authority_epoch.0 != next_epoch
                || grant.old_checkpoint_sha256 != Digest::of(&runtime.snapshot()?).as_str()
            {
                return Err("recovery grant does not bind restored predecessor".into());
            }
            runtime.adopt_authority_epoch(next_epoch)?;
            runtime_config.authority_epoch = next_epoch;
            identity = next_binding;
            let new_cursor = CursorV1 {
                authority_epoch: DecimalId(next_epoch),
                state_hash: runtime
                    .sim()
                    .state_hash()?
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
                ..actual
            };
            takeover_marker = Some(TakeoverMarkerV1 {
                schema_version: 1,
                grant,
                new_cursor,
                checkpoint_sha256: Digest::of(&runtime.snapshot()?).as_str().to_owned(),
            });
        }
    }
    Ok((runtime, runtime_config, identity, report, takeover_marker))
}
