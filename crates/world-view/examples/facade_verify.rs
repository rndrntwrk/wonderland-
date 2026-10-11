//! Read-only receiving tool. The expected SHA-256 must come from a trusted job
//! result/manifest, never from the facade metadata being checked.
use std::{fs::File, io::Read, path::Path};
use wonderland_render_core::{AssetKey, derivatives::fsof::Fsof};
use wonderland_world_view::*;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    // Callers select private regular files; no directory traversal or remote
    // fetch is performed. Services must enforce their own I/O deadlines.
    let file = File::open(path)?;
    let info = file.metadata()?;
    if !info.is_file() || info.len() > limit {
        return Err("facade input is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("facade input grew beyond byte budget".into());
    }
    Ok(bytes)
}
fn digest_from_hex(value: &str) -> Result<AssetKey> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("trusted SHA-256 must be 64 lowercase hexadecimal characters".into());
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(AssetKey(bytes))
}
fn verify_files(
    world_path: &Path,
    facade_path: &Path,
    receipt_path: &Path,
    expected_digest: &str,
    resolution: &str,
) -> Result<Fsof> {
    let trusted = digest_from_hex(expected_digest)?;
    let options = FacadeExportOptions {
        pixels_per_tile: resolution.parse()?,
    };
    if !(1..=8).contains(&options.pixels_per_tile) {
        return Err("pixels per tile must be between 1 and 8".into());
    }
    let world_bytes = read_bounded(world_path, 32 * 1024 * 1024)?;
    let world: WorldDocument = serde_json::from_slice(&world_bytes)?;
    let bytes = read_bounded(facade_path, 16 * 1024 * 1024)?;
    let receipt_bytes = read_bounded(receipt_path, 65_536)?;
    let receipt = std::str::from_utf8(&receipt_bytes)?;
    Ok(verify_world_facade(
        &world, options, trusted, &bytes, receipt,
    )?)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 5 {
        return Err("usage: facade_verify TRUSTED_WORLD.json FACADE.fsof RECEIPT.json TRUSTED_SHA256 PIXELS_PER_TILE".into());
    }
    let decoded = verify_files(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
        args[3].to_str().ok_or("digest must be text")?,
        args[4].to_str().ok_or("resolution must be text")?,
    )?;
    eprintln!(
        "Verified visual facade: floor {}x{}, walls {}x{}; no state was changed.",
        decoded.floor_width, decoded.floor_height, decoded.wall_width, decoded.wall_height
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        directory: std::path::PathBuf,
        digest: String,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "wonderland-facade-verification-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let world = WorldDocument::from_blueprint_xml(
                "<house><size>3</size><world><floors/><walls/></world><objects/></house>",
                "test:verify",
                "fixture",
            )
            .unwrap();
            fs::write(
                directory.join("world.json"),
                serde_json::to_vec(&world).unwrap(),
            )
            .unwrap();
            let mut job = WorldFacadeJob::new(Arc::new(world), Default::default()).unwrap();
            let out = loop {
                if let Some(out) = job.step(128).unwrap() {
                    break out;
                }
            };
            fs::write(directory.join("facade.fsof"), &out.bytes).unwrap();
            fs::write(directory.join("receipt.json"), out.metadata_json).unwrap();
            let digest = Sha256::digest(&out.bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            Self { directory, digest }
        }
        fn verify(&self, hash: &str, resolution: &str) -> Result<Fsof> {
            verify_files(
                &self.directory.join("world.json"),
                &self.directory.join("facade.fsof"),
                &self.directory.join("receipt.json"),
                hash,
                resolution,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
    #[test]
    fn native_receiver_reads_actual_export_without_changing_any_file() {
        let f = Fixture::new();
        let names = ["world.json", "facade.fsof", "receipt.json"];
        let before: Vec<_> = names
            .iter()
            .map(|n| fs::read(f.directory.join(n)).unwrap())
            .collect();
        assert!(f.verify(&f.digest, "4").is_ok());
        let after: Vec<_> = names
            .iter()
            .map(|n| fs::read(f.directory.join(n)).unwrap())
            .collect();
        assert_eq!(before, after);
        assert_eq!(fs::read_dir(&f.directory).unwrap().count(), 3);
    }
    #[test]
    fn native_receiver_rejects_self_reported_digest_and_wrong_resolution() {
        let f = Fixture::new();
        assert!(f.verify(&"0".repeat(64), "4").is_err());
        assert!(f.verify(&f.digest, "2").is_err());
        for invalid in ["", "g", &"A".repeat(64), &"1".repeat(63)] {
            assert!(f.verify(invalid, "4").is_err());
        }
        for invalid in ["0", "9", "65536", "-1", "bad"] {
            assert!(f.verify(&f.digest, invalid).is_err());
        }
    }
    #[test]
    fn native_receiver_rejects_oversized_or_nonregular_inputs_without_writes() {
        let f = Fixture::new();
        fs::write(f.directory.join("receipt.json"), vec![b' '; 65_537]).unwrap();
        assert!(f.verify(&f.digest, "4").is_err());
        fs::remove_file(f.directory.join("receipt.json")).unwrap();
        fs::create_dir(f.directory.join("receipt.json")).unwrap();
        assert!(f.verify(&f.digest, "4").is_err());
    }
    #[test]
    fn native_receiver_reports_missing_input_without_creating_output() {
        let f = Fixture::new();
        fs::remove_file(f.directory.join("facade.fsof")).unwrap();
        assert!(f.verify(&f.digest, "4").is_err());
        assert!(!f.directory.join("facade.fsof").exists());
    }
}
