use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
use wonderland_content_ir::manifest::Digest;

#[path = "support/cooked_fixture.rs"]
mod fixture;

struct Release(PathBuf);
impl Release {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bridge-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        fixture::cook().write_release(&path);
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn seal(&self) -> String {
        let output = run(&[
            "release-prepare".as_ref(),
            self.file("draft.json").as_os_str(),
            "--release-dir".as_ref(),
            self.0.as_os_str(),
            "--output".as_ref(),
            self.file("binding.json").as_os_str(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = std::fs::read(self.file("binding.json")).unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["binding_sha256"], Digest::of(&bytes).as_str());
        Digest::of(&bytes).as_str().to_string()
    }
    fn command(&self, name: &str, sha: &str, extra: &[&std::ffi::OsStr]) -> Output {
        let binding = self.file("binding.json");
        let mut args = vec![
            name.as_ref(),
            binding.as_os_str(),
            "--binding-sha256".as_ref(),
            sha.as_ref(),
            "--release-dir".as_ref(),
            self.0.as_os_str(),
        ];
        args.extend_from_slice(extra);
        run(&args)
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_runtime-bridge"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn commands_prepare_plan_load_and_replay_using_only_selected_cooked_files() {
    let release = Release::new();
    let sha = release.seal();
    let plan = run(&[
        "release-plan".as_ref(),
        release.file("binding.json").as_os_str(),
        "--binding-sha256".as_ref(),
        sha.as_ref(),
        "--manifest".as_ref(),
        release.file("manifest.json").as_os_str(),
    ]);
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["required_packs"].as_array().unwrap().len(), 1);
    let loaded = release.command("release-load", &sha, &[]);
    assert!(
        loaded.status.success(),
        "{}",
        String::from_utf8_lossy(&loaded.stderr)
    );
    let loaded: serde_json::Value = serde_json::from_slice(&loaded.stdout).unwrap();
    assert_eq!(loaded["objects"][0]["guid"], 123);
    let output = release.command(
        "release-replay",
        &sha,
        &[
            "--scenario".as_ref(),
            release.file("scenario.json").as_os_str(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["query"]["stop"]["Completed"], "ReturnTrue");
    assert_eq!(value["query"]["instructions"], 3);
    assert_eq!(
        &value["query"]["temps"].as_array().unwrap()[..2],
        &[serde_json::json!(37), serde_json::json!(41)]
    );
    assert_eq!(value["query"]["snapshot_unchanged"], true);
    assert_eq!(value["replay"]["snapshot_tick"], 2);
    assert_eq!(value["replay"]["tick"], 3);
    assert_eq!(value["replay"]["attributes"], serde_json::json!([41]));
    assert_eq!(value["replay"]["snapshots_match"], true);
    assert_eq!(value["replay"]["effect_dispatches"], 0);
}

#[test]
fn failed_prepare_never_replaces_output_and_untrusted_or_missing_inputs_reject() {
    let release = Release::new();
    let sha = release.seal();
    let original = std::fs::read(release.file("binding.json")).unwrap();
    let output = run(&[
        "release-prepare".as_ref(),
        release.file("draft.json").as_os_str(),
        "--release-dir".as_ref(),
        release.0.as_os_str(),
        "--output".as_ref(),
        release.file("binding.json").as_os_str(),
    ]);
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read(release.file("binding.json")).unwrap(),
        original
    );
    assert!(!release
        .command("release-load", &"0".repeat(64), &[])
        .status
        .success());
    let pack = std::fs::read_dir(&release.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension() == Some("wlp".as_ref()))
        .unwrap();
    std::fs::remove_file(pack).unwrap();
    assert!(!release.command("release-load", &sha, &[]).status.success());
    let output = run(&[
        "release-prepare".as_ref(),
        release.file("draft.json").as_os_str(),
        "--release-dir".as_ref(),
        release.0.as_os_str(),
        "--output".as_ref(),
        release.file("absent.json").as_os_str(),
    ]);
    assert!(!output.status.success());
    assert!(!release.file("absent.json").exists());
}

#[test]
fn bounded_strict_scenario_and_command_arguments_reject_before_replay() {
    let release = Release::new();
    let sha = release.seal();
    let scenario_path = release.file("scenario.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&scenario_path).unwrap()).unwrap();
    for (field, value) in [
        ("instruction_budget", serde_json::json!(0)),
        ("lot_width", serde_json::json!(65)),
        ("unknown", serde_json::json!(false)),
        ("args", serde_json::json!(vec![0; 256])),
        ("initial_attributes", serde_json::json!([])),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        std::fs::write(&scenario_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            !release
                .command(
                    "release-replay",
                    &sha,
                    &["--scenario".as_ref(), scenario_path.as_os_str()]
                )
                .status
                .success(),
            "accepted {field}"
        );
    }
    std::fs::write(&scenario_path, vec![b' '; 65_537]).unwrap();
    assert!(!release
        .command(
            "release-replay",
            &sha,
            &["--scenario".as_ref(), scenario_path.as_os_str()]
        )
        .status
        .success());
    assert!(!release
        .command(
            "release-load",
            &sha,
            &["--binding-sha256".as_ref(), sha.as_ref()]
        )
        .status
        .success());
    assert!(
        !run(&["release-load".as_ref(), Path::new("missing").as_os_str()])
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn observed_symlink_inputs_and_output_parents_are_refused() {
    use std::os::unix::fs::symlink;
    let release = Release::new();
    let alias = release.file("alias");
    symlink(&release.0, &alias).unwrap();
    let output = run(&[
        "release-prepare".as_ref(),
        alias.join("draft.json").as_os_str(),
        "--release-dir".as_ref(),
        release.0.as_os_str(),
        "--output".as_ref(),
        release.file("binding.json").as_os_str(),
    ]);
    assert!(!output.status.success());
    let output = run(&[
        "release-prepare".as_ref(),
        release.file("draft.json").as_os_str(),
        "--release-dir".as_ref(),
        release.0.as_os_str(),
        "--output".as_ref(),
        alias.join("binding.json").as_os_str(),
    ]);
    assert!(!output.status.success());
    assert!(!release.file("binding.json").exists());
}

#[cfg(unix)]
#[test]
fn fifo_binding_and_selected_pack_are_rejected_without_blocking_open() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let release = Release::new();
    let sha = release.seal();
    let pack = std::fs::read_dir(&release.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension() == Some("wlp".as_ref()))
        .unwrap();
    for path in [release.file("binding.json"), pack] {
        let original = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        let mut child = Command::new(env!("CARGO_BIN_EXE_runtime-bridge"))
            .arg("release-load")
            .arg(release.file("binding.json"))
            .arg("--binding-sha256")
            .arg(&sha)
            .arg("--release-dir")
            .arg(&release.0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("opening FIFO blocked instead of rejecting its file type");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
        std::fs::remove_file(&path).unwrap();
        std::fs::write(path, original).unwrap();
    }
}
