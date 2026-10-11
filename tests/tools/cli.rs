#[test]
fn actual_cli_end_to_end_generated_fixtures() {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/tools/cli_e2e.py");
    let result = std::process::Command::new("python3")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_creator"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn actual_cli_guarded_transactions() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/transactions_e2e.py");
    let result = std::process::Command::new("python3")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_creator"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn actual_cli_source_authoring_and_interchange() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/extended_cli_e2e.py");
    let result = std::process::Command::new("python3")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_creator"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}
