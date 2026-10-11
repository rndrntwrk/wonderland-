// SPDX-License-Identifier: MPL-2.0
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static COUNTER: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "wonderland-cooker-cli-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wonderland-asset-cooker"))
        .args(args)
        .output()
        .unwrap()
}
fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}
#[test]
fn real_demo_reproducibility_verify_corruption_subset_plan_and_no_clobber() {
    let t = Temp::new();
    let a = t.0.join("a");
    let b = t.0.join("b");
    let first = run(&["demo", s(&a)]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run(&["demo", s(&b)]);
    assert!(second.status.success());
    let manifest = std::fs::read(a.join("release/manifest.json")).unwrap();
    assert_eq!(
        manifest,
        std::fs::read(b.join("release/manifest.json")).unwrap()
    );
    let decoded: wonderland_content_ir::manifest::AssetManifest =
        serde_json::from_slice(&manifest).unwrap();
    for hash in decoded.packs.keys() {
        assert_eq!(
            std::fs::read(a.join(format!("release/{hash}.wlp"))).unwrap(),
            std::fs::read(b.join(format!("release/{hash}.wlp"))).unwrap()
        );
    }
    assert!(!run(&["demo", s(&a)]).status.success());
    assert_eq!(
        manifest,
        std::fs::read(a.join("release/manifest.json")).unwrap()
    );
    assert!(
        !run(&["cook", s(&a.join("demo.json")), s(&a.join("release"))])
            .status
            .success()
    );
    let hash = wonderland_content_ir::manifest::Digest::of(&manifest);
    assert!(run(&[
        "verify",
        s(&a.join("release")),
        "--manifest-sha256",
        hash.as_str()
    ])
    .status
    .success());
    assert!(!run(&[
        "verify",
        s(&a.join("release")),
        "--manifest-sha256",
        &"0".repeat(64)
    ])
    .status
    .success());
    let plan = run(&[
        "plan",
        s(&a.join("release/manifest.json")),
        "fixture/chunk-42484156-1000",
        "--simulation",
    ]);
    assert!(plan.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["resources"].as_array().unwrap().len(), 3);
    assert_eq!(plan["packs"].as_array().unwrap().len(), 1);
    assert!(!run(&["plan", s(&a.join("release/manifest.json")), "typo"])
        .status
        .success());
    assert!(!run(&[
        "plan",
        s(&a.join("release/manifest.json")),
        "fixture/chunk-42484156-1000",
        "--variant",
        "missing"
    ])
    .status
    .success());
    let file = a.join(format!(
        "release/{}.wlp",
        decoded.packs.keys().next().unwrap()
    ));
    let mut bytes = std::fs::read(&file).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(file, bytes).unwrap();
    assert!(!run(&["verify", s(&a.join("release"))]).status.success());
}
#[test]
fn failures_never_publish_and_inspect_exposes_override_ids() {
    let t = Temp::new();
    let a = t.0.join("a");
    assert!(run(&["demo", s(&a)]).status.success());
    let inspect = run(&["inspect", s(&a.join("authored.iff")), "iff", "fixture"]);
    assert!(inspect.status.success());
    let text = String::from_utf8(inspect.stdout).unwrap();
    assert!(text.contains("fixture/chunk-5a5a5a5a-0007"));
    let mut spec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(a.join("demo.json")).unwrap()).unwrap();
    spec["overrides"][0]["id"] = "fixture/typo".into();
    std::fs::write(a.join("bad.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    let output = t.0.join("failure");
    assert!(!run(&["cook", s(&a.join("bad.json")), s(&output)])
        .status
        .success());
    assert!(!output.exists());
    spec["overrides"][0]["id"] = "fixture/chunk-42484156-1000".into();
    spec["sources"][0]["path"] = "../outside.iff".into();
    std::fs::write(a.join("bad.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    assert!(!run(&["cook", s(&a.join("bad.json")), s(&output)])
        .status
        .success());
    assert!(!output.exists());
    spec["sources"][0]["path"] = "authored.iff".into();
    spec["sources"][0]["provenance"]["redistribution"] = "restricted".into();
    std::fs::write(a.join("bad.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    assert!(
        !run(&["cook", s(&a.join("bad.json")), s(&output), "--public"])
            .status
            .success()
    );
    assert!(!output.exists());
    #[cfg(unix)]
    {
        spec["sources"][0]["path"] = "linked.iff".into();
        std::os::unix::fs::symlink(a.join("authored.iff"), a.join("linked.iff")).unwrap();
        std::fs::write(a.join("bad.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
        assert!(!run(&["cook", s(&a.join("bad.json")), s(&output)])
            .status
            .success());
        assert!(!output.exists());
    }
}
