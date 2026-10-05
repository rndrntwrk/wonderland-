// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Local, private workspace. Paths are relative and every existing component must be a non-symlink.
/// Protect the root from concurrent untrusted renames; std does not expose descriptor-relative traversal.
pub struct Workspace {
    root: PathBuf,
    pub max_file_bytes: usize,
}
impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref();
        if fs::symlink_metadata(root)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("workspace root must not be a symlink".into());
        }
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("workspace root must be a directory".into());
        }
        Ok(Self {
            root,
            max_file_bytes: MAX_FILE_BYTES,
        })
    }
    fn resolve(&self, path: &Path, allow_missing_leaf: bool) -> Result<PathBuf, String> {
        if path.as_os_str().is_empty() {
            return Err("empty path".into());
        }
        let parts: Vec<_> = path.components().collect();
        if parts.iter().any(|c| !matches!(c, Component::Normal(_))) {
            return Err("only normal relative path components are allowed".into());
        }
        let mut p = self.root.clone();
        for (i, c) in parts.iter().enumerate() {
            p.push(c.as_os_str());
            match fs::symlink_metadata(&p) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err("symlink path component refused".into())
                }
                Ok(m) if i + 1 < parts.len() && !m.is_dir() => {
                    return Err("parent must be a directory".into())
                }
                Ok(_) => {}
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        && allow_missing_leaf
                        && i + 1 == parts.len() => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(p)
    }
    pub fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, String> {
        self.read_limited(path, self.max_file_bytes)
    }
    /// Apply a narrower limit before allocation, retaining the workspace ceiling
    /// and the same path/type checks used for ordinary file reads.
    pub fn read_limited(
        &self,
        path: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        let max_bytes = max_bytes.min(self.max_file_bytes);
        let p = self.resolve(path.as_ref(), false)?;
        if !fs::symlink_metadata(&p)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("input must be a regular file".into());
        }
        let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
        let m = f.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() {
            return Err("input must be a regular file".into());
        }
        if m.len() > max_bytes as u64 {
            return Err("input byte limit exceeded".into());
        }
        let length = usize::try_from(m.len()).map_err(|_| "input length overflow")?;
        // Allocate only the checked metadata length. read_to_end can double a
        // Vec's capacity beyond the caller's remaining allocation budget.
        let mut bytes = vec![0; length];
        f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        // Detect growth without reserving another byte (and triggering geometric
        // Vec growth). Truncation already fails read_exact; neither is published.
        let mut extra = [0u8; 1];
        if f.read(&mut extra).map_err(|e| e.to_string())? != 0 {
            return Err("input changed size while reading".into());
        }
        Ok(bytes)
    }
    fn check_output_type(path: &Path) -> Result<(), String> {
        match fs::symlink_metadata(path) {
            Ok(m) if m.is_file() => Ok(()),
            Ok(_) => Err("output must be a regular file".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn create_temporary(
        target: &Path,
        mut next_serial: impl FnMut() -> u64,
    ) -> Result<(PathBuf, fs::File), String> {
        let parent = target.parent().ok_or("output has no parent")?;
        let mut created = None;
        for _ in 0..64 {
            let temp = parent.join(format!(
                ".creator-{}-{}.tmp",
                std::process::id(),
                next_serial()
            ));
            // A caller may choose a destination that resembles our temporary namespace.
            // Never expose that destination before the complete file is ready to publish.
            if temp == target {
                continue;
            }
            match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(f) => {
                    created = Some((temp, f));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        created.ok_or_else(|| "cannot allocate unique temporary file".into())
    }

    pub fn write_atomic(&self, path: impl AsRef<Path>, bytes: &[u8]) -> Result<PathBuf, String> {
        if bytes.len() > self.max_file_bytes {
            return Err("output byte limit exceeded".into());
        }
        let path = path.as_ref();
        let target = self.resolve(path, true)?;
        Self::check_output_type(&target)?;
        #[cfg(unix)]
        let parent = target.parent().ok_or("output has no parent")?.to_path_buf();
        let (temp, mut f) =
            Self::create_temporary(&target, || SERIAL.fetch_add(1, Ordering::Relaxed))?;
        let result = (|| {
            f.write_all(bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            // Recheck the boundary immediately before publishing a complete file.
            self.resolve(path, true)?;
            Self::check_output_type(&target)?;
            fs::rename(&temp, &target).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            fs::File::open(&parent)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            Ok(target)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_collision_keeps_final_path_unpublished_until_complete() {
        let root = std::env::temp_dir().join(format!(
            "creator-atomic-collision-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let target = root.join(format!(".creator-{}-0.tmp", std::process::id()));
        let serial = AtomicU64::new(0);
        let (temp, mut file) =
            Workspace::create_temporary(&target, || serial.fetch_add(1, Ordering::Relaxed))
                .unwrap();
        let unpublished = !target.exists() && temp != target;
        if !unpublished {
            let _ = fs::remove_file(&temp);
            let _ = fs::remove_dir(&root);
        }
        assert!(
            unpublished,
            "temporary allocation exposed the final path before any bytes were written"
        );
        file.write_all(b"complete output").unwrap();
        file.sync_all().unwrap();
        assert!(
            !target.exists(),
            "writing the temporary file must not publish the destination"
        );
        fs::rename(&temp, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"complete output");
        fs::remove_dir_all(&root).unwrap();
    }
}
