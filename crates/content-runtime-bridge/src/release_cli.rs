//! File I/O for the portable cooked library. Paths must remain stable during a
//! command; observed symbolic links and parent traversal are rejected.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use wonderland_content_ir::manifest::Digest;
use wonderland_content_runtime_bridge::{
    cooked::{
        json_report, CookedLoadLimits, PackBytes, PackRequest, PreparedDraft, PreparedRelease,
    },
    cooked_replay::{replay_json, ReplayScenarioV1, MAX_SCENARIO_BYTES},
};

pub const HELP: &str = "runtime-bridge release-prepare DRAFT --release-dir DIR --output BINDING\nruntime-bridge release-plan BINDING --binding-sha256 SHA256 --manifest MANIFEST\nruntime-bridge release-load BINDING --binding-sha256 SHA256 --release-dir DIR\nruntime-bridge release-replay BINDING --binding-sha256 SHA256 --release-dir DIR --scenario SCENARIO\n\nRelease directories contain manifest.json and digest-named .wlp files.\nPreparation verifies all selected packs and seals the actual runtime descriptor.\nSupply the final binding hash from an independently trusted channel.\nReplay is an explicit empty-lot replica harness, with one query and one whole tick.";

pub fn run(args: &[OsString]) -> Result<Vec<u8>, String> {
    let command = args
        .first()
        .and_then(|arg| arg.to_str())
        .ok_or("missing release command")?;
    let input = args.get(1).ok_or("missing binding or draft path")?;
    let allowed: &[&str] = match command {
        "release-prepare" => &["--release-dir", "--output"],
        "release-plan" => &["--binding-sha256", "--manifest"],
        "release-load" => &["--binding-sha256", "--release-dir"],
        "release-replay" => &["--binding-sha256", "--release-dir", "--scenario"],
        _ => return Err("unknown release command".into()),
    };
    if args.len() != 2 + 2 * allowed.len() {
        return Err("missing or unexpected release arguments".into());
    }
    let mut options = BTreeMap::new();
    for pair in args[2..].as_chunks::<2>().0 {
        let key = pair[0].to_str().ok_or("non-text option name")?;
        if !allowed.contains(&key) || options.insert(key, pair[1].as_os_str()).is_some() {
            return Err("unknown or duplicate release option".into());
        }
    }
    if options.len() != allowed.len() {
        return Err("missing release option".into());
    }
    let limits = CookedLoadLimits::default();
    let manifest_path = if command == "release-plan" {
        PathBuf::from(options["--manifest"])
    } else {
        let directory = Path::new(options["--release-dir"]);
        real_path(directory)?;
        if !directory.is_dir() {
            return Err("release directory is not a directory".into());
        }
        directory.join("manifest.json")
    };
    let bytes = read_bounded(Path::new(input), limits.max_binding_bytes)?;
    let manifest = read_bounded(&manifest_path, limits.manifest.max_manifest_bytes)?;
    if command == "release-prepare" {
        let prepared = PreparedDraft::from_json(&bytes, &manifest, &limits)?;
        let payloads = read_packs(
            Path::new(options["--release-dir"]),
            prepared.required_packs(),
            &limits,
        )?;
        let refs = pack_refs(prepared.required_packs(), &payloads);
        let sealed = prepared.seal(&refs, &limits)?.canonical_bytes(&limits)?;
        #[derive(Serialize)]
        struct Report {
            binding_sha256: Digest,
            binding_bytes: usize,
            manifest_sha256: Digest,
        }
        let report = json_report(&Report {
            binding_sha256: Digest::of(&sealed),
            binding_bytes: sealed.len(),
            manifest_sha256: Digest::of(&manifest),
        })?;
        write_new(Path::new(options["--output"]), &sealed)?;
        return Ok(report);
    }
    let sha = options["--binding-sha256"]
        .to_str()
        .ok_or("binding SHA must be text")?;
    if sha.len() != 64 {
        return Err("binding SHA must contain 64 lowercase hexadecimal digits".into());
    }
    let expected = Digest::try_from(sha.to_owned())
        .map_err(|_| "binding SHA must contain 64 lowercase hexadecimal digits")?;
    let prepared = PreparedRelease::from_binding(&bytes, &expected, &manifest, &limits)?;
    if command == "release-plan" {
        return prepared.plan_report();
    }
    let scenario = if command == "release-replay" {
        Some(ReplayScenarioV1::from_json(&read_bounded(
            Path::new(options["--scenario"]),
            MAX_SCENARIO_BYTES,
        )?)?)
    } else {
        None
    };
    let payloads = read_packs(
        Path::new(options["--release-dir"]),
        prepared.required_packs(),
        &limits,
    )?;
    let loaded = prepared.load(&pack_refs(prepared.required_packs(), &payloads), &limits)?;
    match scenario {
        Some(request) => replay_json(loaded, &request),
        None => json_report(&loaded.report),
    }
}

fn pack_refs<'a>(requests: &'a [PackRequest], payloads: &'a [Vec<u8>]) -> Vec<PackBytes<'a>> {
    requests
        .iter()
        .zip(payloads)
        .map(|(request, bytes)| PackBytes {
            digest: &request.digest,
            bytes,
        })
        .collect()
}

fn read_packs(
    directory: &Path,
    requests: &[PackRequest],
    limits: &CookedLoadLimits,
) -> Result<Vec<Vec<u8>>, String> {
    let total = requests.iter().try_fold(0u64, |sum, request| {
        sum.checked_add(request.byte_len)
            .ok_or("selected pack byte overflow")
    })?;
    if total > limits.max_total_pack_bytes {
        return Err("selected pack byte limit".into());
    }
    let mut result = Vec::with_capacity(requests.len());
    for request in requests {
        let cap = usize::try_from(request.byte_len)
            .map_err(|_| "pack length does not fit this platform")?;
        let bytes = read_bounded(
            &directory.join(format!("{}.wlp", request.digest.as_str())),
            cap,
        )?;
        if bytes.len() != cap {
            return Err("selected pack file length differs from manifest".into());
        }
        result.push(bytes);
    }
    Ok(result)
}

fn real_path(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return Err("unsupported path prefix".into()),
            Component::ParentDir => return Err("parent path traversal is not allowed".into()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => {
                current.push(part);
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|_| "input path is missing or inaccessible")?;
                if metadata.file_type().is_symlink() {
                    return Err("symbolic link paths are not allowed".into());
                }
            }
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    real_path(path)?;
    // Opening a FIFO can block before File::metadata is reachable. Reject
    // special leaf types first, then recheck the opened file below.
    if !fs::symlink_metadata(path)
        .map_err(|_| "input metadata is inaccessible")?
        .file_type()
        .is_file()
    {
        return Err("input must be a regular file".into());
    }
    let mut file = File::open(path).map_err(|_| "input file cannot be opened")?;
    let metadata = file
        .metadata()
        .map_err(|_| "input metadata is inaccessible")?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    let len = usize::try_from(metadata.len())
        .map_err(|_| "input file length does not fit this platform")?;
    if len > cap {
        return Err("input file exceeds its byte limit".into());
    }
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes)
        .map_err(|_| "input file changed or could not be read")?;
    let mut extra = [0];
    if file
        .read(&mut extra)
        .map_err(|_| "input file could not be read")?
        != 0
        || file
            .metadata()
            .map_err(|_| "input metadata is inaccessible")?
            .len()
            != metadata.len()
    {
        return Err("input file length changed while reading".into());
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.file_name().is_none() {
        return Err("output must name a new file".into());
    }
    real_path(path.parent().unwrap_or_else(|| Path::new(OsStr::new("."))))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "output already exists or cannot be created")?;
    if file.write_all(bytes).and_then(|_| file.flush()).is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err("new binding could not be written".into());
    }
    Ok(())
}
