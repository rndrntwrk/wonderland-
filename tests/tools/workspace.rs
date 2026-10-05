use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use wonderland_creator::{sha256, Workspace};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "creator-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn hashes_use_standard_sha256() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
#[test]
fn root_boundary_and_atomic_write() {
    let tmp = Temp::new();
    let ws = Workspace::new(&tmp.0).unwrap();
    fs::write(tmp.0.join("out"), b"before").unwrap();
    assert!(ws.read("../escape").is_err());
    assert!(ws.write_atomic("../escape", b"bad").is_err());
    ws.write_atomic("out", b"after").unwrap();
    assert_eq!(fs::read(tmp.0.join("out")).unwrap(), b"after");
    assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn rejects_symlink_reads_outputs_and_parents() {
    let tmp = Temp::new();
    let outside = Temp::new();
    fs::write(outside.0.join("asset"), b"private").unwrap();
    std::os::unix::fs::symlink(outside.0.join("asset"), tmp.0.join("file-link")).unwrap();
    std::os::unix::fs::symlink(&outside.0, tmp.0.join("dir-link")).unwrap();
    let ws = Workspace::new(&tmp.0).unwrap();
    assert!(ws.read("file-link").is_err());
    assert!(ws.read("dir-link/asset").is_err());
    assert!(ws.write_atomic("file-link", b"bad").is_err());
    assert!(ws.write_atomic("dir-link/asset", b"bad").is_err());
    assert_eq!(fs::read(outside.0.join("asset")).unwrap(), b"private");
}
#[test]
fn bounded_read_and_failed_output_preserve_existing_file() {
    let tmp = Temp::new();
    let mut ws = Workspace::new(&tmp.0).unwrap();
    ws.max_file_bytes = 3;
    fs::write(tmp.0.join("large"), b"1234").unwrap();
    fs::write(tmp.0.join("out"), b"old").unwrap();
    assert!(ws.read("large").is_err());
    assert!(ws.write_atomic("out", b"four").is_err());
    assert_eq!(fs::read(tmp.0.join("out")).unwrap(), b"old");
    assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 2);
    assert!(ws.read(tmp.0.join("out")).is_err());
}
