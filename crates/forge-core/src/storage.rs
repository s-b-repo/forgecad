//! Publish completed file contents within the destination directory.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Without `overwrite`, an existing file or symlink is never replaced, even if
/// it appears concurrently. A staged file is atomically linked/renamed only
/// after all bytes have been written and synced. No system temporary directory
/// is used. Filesystems without hard links return an error for no-clobber saves.
pub fn write_file(path: &Path, bytes: &[u8], overwrite: bool) -> io::Result<()> {
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "An output filename is required",
        ));
    }
    if !overwrite && fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("Output {} already exists", path.display()),
        ));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = None;
    for _ in 0..100 {
        let candidate = parent.join(format!(
            ".forge-write-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                staged = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let (staging, mut file) = staged.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot allocate staged output file",
        )
    })?;
    let result: io::Result<()> = (|| {
        if overwrite {
            if let Ok(metadata) = fs::metadata(path) {
                file.set_permissions(metadata.permissions())?;
            }
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if overwrite {
            fs::rename(&staging, path)?;
        } else {
            fs::hard_link(&staging, path)?;
        }
        Ok(())
    })();
    let cleanup = fs::remove_file(&staging);
    result?;
    if let Err(error) = cleanup {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error);
        }
    }
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestFiles(std::path::PathBuf);
    impl TestFiles {
        fn new() -> Self {
            let p = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/storage-tests")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TestFiles {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn existing_output_is_preserved_until_overwrite_is_explicit() {
        let files = TestFiles::new();
        let p = files.0.join("design");
        write_file(&p, b"original", false).unwrap();
        assert_eq!(
            write_file(&p, b"replacement", false).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&p).unwrap(), b"original");
        write_file(&p, b"replacement", true).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);
    }
    #[test]
    fn publication_failure_does_not_leave_staged_files() {
        let files = TestFiles::new();
        let p = files.0.join("directory");
        fs::create_dir(&p).unwrap();
        assert!(write_file(&p, b"data", true).is_err());
        assert!(p.is_dir());
        assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn dangling_symlink_is_not_followed_or_overwritten_implicitly() {
        let files = TestFiles::new();
        let p = files.0.join("link");
        let target = files.0.join("missing");
        std::os::unix::fs::symlink(&target, &p).unwrap();
        assert_eq!(
            write_file(&p, b"data", false).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(!target.exists());
    }
}
