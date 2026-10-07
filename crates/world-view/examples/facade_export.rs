//! Native visual-derivative worker for a normalized WorldDocument JSON.
//! Usage: cargo run -p wonderland-world-view --example facade_export -- INPUT.json NEW_DIRECTORY
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use wonderland_world_view::{WorldDocument, WorldFacadeJob, WorldFacadeOutput};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_INPUT: u64 = 32 * 1024 * 1024;
fn read_document(path: &Path) -> Result<WorldDocument> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_INPUT {
        return Err("source input byte budget".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("source input byte budget".into());
    }
    let world: WorldDocument = serde_json::from_slice(&bytes)?;
    world.validate()?;
    Ok(world)
}
fn write_file(path: &Path, bytes: &[u8], created: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    created.push(path.to_path_buf());
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn export(input: &Path, directory: &Path) -> Result<()> {
    // Refuse existing destinations before expensive rendering, and recheck
    // atomically with create_dir after rendering to prevent clobber races.
    if directory.symlink_metadata().is_ok() {
        return Err("output directory must not exist".into());
    }
    let mut job = WorldFacadeJob::new(Arc::new(read_document(input)?), Default::default())?;
    let output = loop {
        if let Some(output) = job.step(64)? {
            break output;
        }
    };
    fs::create_dir(directory)?;
    publish(directory, &output)
}
fn publish(directory: &Path, output: &WorldFacadeOutput) -> Result<()> {
    let image = directory.join("facade.fsof");
    let receipt = directory.join("metadata.json");
    let mut created = Vec::new();
    let result = (|| {
        write_file(&image, &output.bytes, &mut created)?;
        // The metadata is the last publication marker. Consumers use only a
        // complete pair with a matching digest, never an in-progress folder.
        write_file(&receipt, output.metadata_json.as_bytes(), &mut created)?;
        Ok(())
    })();
    if result.is_err() {
        // Remove only files whose exclusive create succeeded. A competing
        // writer may own either name; failed create_new must not delete it.
        for path in created {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(directory);
    }
    result
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: facade_export INPUT.json NEW_DIRECTORY".into());
    }
    export(Path::new(&args[0]), Path::new(&args[1]))?;
    eprintln!("Facade and digest metadata written; no game state or database was changed.");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "wonderland-facade-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
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
    fn input(temp: &Temp) -> std::path::PathBuf {
        let world = WorldDocument::from_blueprint_xml(
            "<house><size>3</size><world><floors/><walls/></world><objects/></house>",
            "test:worker",
            "fixture",
        )
        .unwrap();
        let path = temp.0.join("world.json");
        fs::write(&path, serde_json::to_vec(&world).unwrap()).unwrap();
        path
    }
    #[test]
    fn writes_real_fsof_and_hash_metadata() {
        use sha2::{Digest, Sha256};
        let temp = Temp::new();
        let src = input(&temp);
        let dest = temp.0.join("output");
        export(&src, &dest).unwrap();
        let bytes = fs::read(dest.join("facade.fsof")).unwrap();
        wonderland_render_core::derivatives::fsof::Fsof::decode(&bytes, Default::default())
            .unwrap();
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(dest.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(meta["sha256"], format!("{:x}", Sha256::digest(&bytes)));
    }
    #[test]
    fn never_overwrites_an_existing_directory() {
        let temp = Temp::new();
        let src = input(&temp);
        let dest = temp.0.join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("keep"), b"unchanged").unwrap();
        assert!(export(&src, &dest).is_err());
        assert_eq!(fs::read(dest.join("keep")).unwrap(), b"unchanged");
    }
    #[test]
    fn malformed_input_does_not_create_a_destination() {
        let temp = Temp::new();
        let src = temp.0.join("invalid.json");
        fs::write(&src, b"{}").unwrap();
        let dest = temp.0.join("output");
        assert!(export(&src, &dest).is_err());
        assert!(!dest.exists());
    }
    #[test]
    fn publication_collision_preserves_a_file_the_worker_did_not_create() {
        let temp = Temp::new();
        let dest = temp.0.join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("metadata.json"), b"another owner's data").unwrap();
        let output = WorldFacadeOutput {
            bytes: b"test image".to_vec(),
            metadata_json: "{}".into(),
            source_hash: "0".repeat(64),
        };
        assert!(publish(&dest, &output).is_err());
        assert_eq!(
            fs::read(dest.join("metadata.json")).unwrap(),
            b"another owner's data"
        );
        assert!(
            !dest.join("facade.fsof").exists(),
            "our partial image must be removed"
        );
    }
    #[test]
    fn rejects_oversized_input_without_reading_it_into_memory() {
        let temp = Temp::new();
        let src = temp.0.join("large.json");
        File::create(&src).unwrap().set_len(MAX_INPUT + 1).unwrap();
        assert!(read_document(&src).is_err());
    }
}
