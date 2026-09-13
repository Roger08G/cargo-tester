use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use atomic_write_file::AtomicWriteFile;
use fs2::FileExt;

pub(crate) const MAX_PERSISTED_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) struct OutputLock {
    _file: File,
}

impl OutputLock {
    pub(crate) fn acquire(output_path: &Path) -> Result<Self> {
        let lock_path = lock_path(output_path)?;
        let parent = lock_path
            .parent()
            .context("output lock path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        ensure_regular_or_missing(&lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        if !file.metadata()?.is_file() {
            bail!("output lock must be a regular file");
        }
        let started = Instant::now();
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => break,
                Err(error)
                    if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                {
                    if started.elapsed() >= Duration::from_secs(10)
                        || crate::process::Cancellation::passive().is_cancelled()
                    {
                        bail!(
                            "timed out or cancelled while acquiring output lock {}",
                            lock_path.display()
                        );
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", lock_path.display()));
                }
            }
        }
        Ok(Self { _file: file })
    }
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().context("output file path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    ensure_regular_or_missing(path)?;

    let mut file = AtomicWriteFile::options()
        .open(path)
        .with_context(|| format!("failed to open atomic writer for {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.commit()
        .with_context(|| format!("failed to commit {}", path.display()))
}

pub(crate) fn atomic_write_state(path: &Path, contents: &[u8]) -> Result<()> {
    if contents.len() as u64 > MAX_PERSISTED_BYTES {
        bail!(
            "{} exceeds the {} byte state limit; existing state was preserved",
            path.display(),
            MAX_PERSISTED_BYTES
        );
    }
    atomic_write(path, contents)
}

fn lock_path(output_path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(output_path)?;
    // The lock lives beside its data: aliases and case variants necessarily
    // acquire the same filesystem lock, with no shared-temp lock namespace.
    Ok(fs::canonicalize(output_path)?.join(".cargo-tester.lock"))
}

fn ensure_regular_or_missing(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            bail!("refusing non-regular output file {}", path.display())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

pub(crate) fn read_text(path: &Path, limit: u64) -> Result<Option<String>> {
    ensure_regular_or_missing(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        bail!(
            "{} is not a regular file within the {} byte limit",
            path.display(),
            limit
        );
    }
    let mut contents = String::new();
    file.take(limit + 1).read_to_string(&mut contents)?;
    if contents.len() as u64 > limit {
        bail!("{} exceeded its byte limit", path.display());
    }
    Ok(Some(contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_output_paths_use_one_lock() {
        let root = std::env::temp_dir().join(format!("cargo-tester-lock-{}", std::process::id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        assert_eq!(
            lock_path(&root).unwrap(),
            lock_path(&root.join("nested/..")).unwrap()
        );
        fs::remove_dir(root.join("nested")).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn oversized_state_preserves_existing_file() {
        let path = std::env::temp_dir().join(format!(
            "cargo-tester-state-limit-{}.json",
            std::process::id()
        ));
        fs::write(&path, "existing state").unwrap();
        assert!(atomic_write_state(&path, &vec![b'x'; MAX_PERSISTED_BYTES as usize + 1]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "existing state");
        fs::remove_file(path).unwrap();
    }
}
