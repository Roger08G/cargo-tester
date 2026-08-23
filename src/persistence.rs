use std::{
    collections::hash_map::DefaultHasher,
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use atomic_write_file::AtomicWriteFile;
use fs2::FileExt;

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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        FileExt::lock_exclusive(&file)
            .with_context(|| format!("failed to lock {}", lock_path.display()))?;
        Ok(Self { _file: file })
    }
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().context("output file path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let mut file = AtomicWriteFile::options()
        .open(path)
        .with_context(|| format!("failed to open atomic writer for {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.commit()
        .with_context(|| format!("failed to commit {}", path.display()))
}

fn lock_path(output_path: &Path) -> Result<PathBuf> {
    let absolute = if output_path.is_absolute() {
        output_path.to_owned()
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory for output lock")?
            .join(output_path)
    };
    let mut hasher = DefaultHasher::new();
    absolute.hash(&mut hasher);
    let lock_name = format!("{:016x}.lock", hasher.finish());
    Ok(std::env::temp_dir()
        .join("cargo-tester-locks")
        .join(lock_name))
}
