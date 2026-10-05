// SPDX-License-Identifier: MPL-2.0
//! Local content pipeline operator; no network calls or simulation execution.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use wonderland_asset_cooker::{
    fs_io,
    interchange::{demo_iff, import_spec},
    manifest::{cook_resources, CookLimits},
    registry::{CookSpec, ResourceOverride, Rights, SourceFormat, SourceSpec, SOURCE_BASELINE},
};
use wonderland_content_ir::manifest::{
    AssetManifest, Digest, LoadPhase, ManifestError, ManifestResult, Origin, Redistribution,
};
fn err(e: impl std::fmt::Display) -> ManifestError {
    ManifestError(e.to_string())
}
fn summary(manifest: &AssetManifest, hash: &Digest) {
    println!(
        "verified {} resources in {} packs; manifest sha256 {}",
        manifest.resources.len(),
        manifest.packs.len(),
        hash
    );
}
fn cook(spec_path: &Path, output: &Path, public: bool, limits: &CookLimits) -> ManifestResult<()> {
    let bytes = fs_io::read_bounded(spec_path, limits.manifest.max_manifest_bytes)?;
    let spec = CookSpec::from_json(&bytes, limits)?;
    let root = spec_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let imported = import_spec(&spec, root, limits)?;
    let content = cook_resources(
        &spec.source_baseline,
        spec.tuning_version,
        &imported.resources,
        limits,
    )?;
    if public {
        content.manifest.require_public_distribution()?;
    }
    let report = imported
        .report
        .canonical_bytes(limits.manifest.max_manifest_bytes)?;
    let hash = fs_io::write_release(output, &content, &report, limits)?;
    let (verified, verified_hash) = fs_io::verify_release(output, Some(&hash), limits)?;
    summary(&verified, &verified_hash);
    Ok(())
}
fn demo(output: &Path, limits: &CookLimits) -> ManifestResult<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs_io::reject_symlinks(parent)?;
    std::fs::create_dir(output).map_err(err)?;
    let result = (|| {
        std::fs::write(output.join("authored.iff"), demo_iff(&limits.legacy)?).map_err(err)?;
        let spec = CookSpec {
            schema_version: 1,
            fixture_only: true,
            source_baseline: SOURCE_BASELINE.into(),
            tuning_version: Digest::of(b"authored fixture tuning v1"),
            sources: vec![SourceSpec {
                id: "fixture".into(),
                path: "authored.iff".into(),
                format: SourceFormat::Iff,
                source_name: Some("Authored.iff".into()),
                pack_group: "demo".into(),
                provenance: Rights {
                    origin: Origin::Authored,
                    license: Some("CC0-1.0".into()),
                    redistribution: Redistribution::Allowed,
                },
                patches: Vec::new(),
                semiglobal: None,
                global: None,
                tuning: Default::default(),
                resolver_variant: String::new(),
                locale_selection: Default::default(),
            }],
            overrides: vec![
                ResourceOverride {
                    id: "fixture/chunk-42484156-1000".into(),
                    dependencies: vec![
                        "fixture/chunk-42434f4e-1000".into(),
                        "fixture/resolved-tuning".into(),
                    ],
                    simulation_critical: true,
                    locale: None,
                    variants: BTreeSet::new(),
                },
                ResourceOverride {
                    id: "fixture/chunk-42434f4e-1000".into(),
                    dependencies: Vec::new(),
                    simulation_critical: true,
                    locale: None,
                    variants: BTreeSet::new(),
                },
                ResourceOverride {
                    id: "fixture/resolved-tuning".into(),
                    dependencies: vec!["fixture/chunk-42434f4e-1000".into()],
                    simulation_critical: true,
                    locale: None,
                    variants: BTreeSet::new(),
                },
            ],
        };
        std::fs::write(
            output.join("demo.json"),
            serde_json::to_vec_pretty(&spec).map_err(err)?,
        )
        .map_err(err)?;
        cook(
            &output.join("demo.json"),
            &output.join("release"),
            true,
            limits,
        )?;
        let (manifest, _) = fs_io::verify_release(&output.join("release"), None, limits)?;
        let plan = manifest.load_plan(
            &["fixture/chunk-42484156-1000".into()],
            LoadPhase::Simulation,
            None,
            &BTreeSet::new(),
            &limits.manifest,
        )?;
        if plan.resources.len() != 3 {
            return Err(err("fixture readiness closure mismatch"));
        }
        println!("{}", serde_json::to_string_pretty(&plan).map_err(err)?);
        println!("Authored content pipeline fixture only; this is not a playable simulation.");
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(output);
    }
    result
}
fn parse_format(value: &str) -> ManifestResult<SourceFormat> {
    serde_json::from_value(serde_json::Value::String(value.into())).map_err(err)
}
fn run() -> ManifestResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let limits = CookLimits::default();
    match args.first().map(String::as_str) {
        Some("cook") if args.len()==3 || (args.len()==4 && args[3]=="--public") => cook(Path::new(&args[1]),Path::new(&args[2]),args.len()==4,&limits),
        Some("verify") if args.len()==2 || (args.len()==4 && args[2]=="--manifest-sha256") => {
            let expected=if args.len()==4{Some(Digest::try_from(args[3].clone())?)}else{None};
            let (manifest,hash)=fs_io::verify_release(Path::new(&args[1]),expected.as_ref(),&limits)?;summary(&manifest,&hash);Ok(())
        }
        Some("plan") if args.len()>=3 => {
            let manifest=AssetManifest::from_json(&fs_io::read_bounded(Path::new(&args[1]),limits.manifest.max_manifest_bytes)?,&limits.manifest)?;
            let mut phase=LoadPhase::All;let mut variant=None;let mut roots=Vec::new();let mut i=2;
            while i<args.len(){match args[i].as_str(){"--simulation"=>{phase=LoadPhase::Simulation;i+=1;},"--variant"=>{if variant.is_some()||i+1>=args.len(){return Err(err("--variant requires one name"));}variant=Some(args[i+1].as_str());i+=2;},value if value.starts_with('-')=>return Err(err("unknown plan option")),_=>{roots.push(args[i].clone());i+=1;}}}
            if roots.is_empty(){return Err(err("plan needs resource roots"));}
            if variant.is_some_and(|v| !manifest.resources.values().any(|r|r.variants.contains(v))) { return Err(err("unknown manifest variant")); }
            let plan=manifest.load_plan(&roots,phase,variant,&BTreeSet::new(),&limits.manifest)?;
            println!("{}",serde_json::to_string_pretty(&plan).map_err(err)?);Ok(())
        }
        Some("inspect") if args.len()==3 || args.len()==4 => {
            let path=PathBuf::from(&args[1]);let root=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            let filename=path.file_name().ok_or_else(||err("inspect requires a filename"))?.to_str().ok_or_else(||err("inspect filename must be UTF-8"))?;
            let id=args.get(3).cloned().unwrap_or_else(||"inspect".into());
            let spec=CookSpec{schema_version:1,fixture_only:false,source_baseline:SOURCE_BASELINE.into(),tuning_version:Digest::of(b"inspection"),sources:vec![SourceSpec{id,path:filename.into(),format:parse_format(&args[2])?,source_name:None,pack_group:"inspect".into(),provenance:Rights{origin:Origin::UserImported,license:None,redistribution:Redistribution::Unknown},patches:Vec::new(),semiglobal:None,global:None,tuning:Default::default(),resolver_variant:String::new(),locale_selection:Default::default()}],overrides:Vec::new()};
            let imported=import_spec(&spec,root,&limits)?;
            println!("{}",std::str::from_utf8(&imported.report.canonical_bytes(limits.manifest.max_manifest_bytes)?).map_err(err)?);Ok(())
        }
        Some("demo") if args.len()==2=>demo(Path::new(&args[1]),&limits),
        _=>Err(err("usage: wonderland-asset-cooker cook <spec.json> <new-dir> [--public] | verify <dir> [--manifest-sha256 <digest>] | plan <manifest.json> <resource-id>... [--simulation] [--variant <name>] | inspect <file> <format> [source-id] | demo <new-dir>")),
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("asset-cooker: {e}");
        std::process::exit(1);
    }
}
