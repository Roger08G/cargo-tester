use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, Result};
use regex::Regex;

const MAX_SOURCE_FILE_BYTES: u64 = 1024 * 1024;
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;

static TEST_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?m)^\s*#\s*\[\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*test",
        r"(?:\s*\([^]]*\))?\s*\]\s*",
        r"(?:#\s*\[[^]]+\]\s*)*",
        r"(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+",
        r"([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\(",
    ))
    .expect("test function regex must be valid")
});

#[derive(Debug)]
pub struct SourceIndex {
    root: PathBuf,
    locations_by_test: HashMap<String, Vec<SourceLocation>>,
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: String,
    pub line: Option<usize>,
    pub source_line: Option<String>,
}

impl SourceIndex {
    pub fn build(root: PathBuf) -> Result<Self> {
        let mut rust_files = Vec::new();
        collect_rust_files(&root, &mut rust_files)?;
        rust_files.sort_unstable();

        let mut locations_by_test = HashMap::new();
        let mut indexed_bytes = 0_usize;

        for path in rust_files {
            let Some(contents) = read_source_file(&path) else {
                continue;
            };
            indexed_bytes = indexed_bytes.saturating_add(contents.len());
            if indexed_bytes > MAX_INDEX_BYTES {
                break;
            }
            let mut line_starts = vec![0];
            line_starts.extend(contents.match_indices('\n').map(|(index, _)| index + 1));
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();

            for capture in TEST_FUNCTION.captures_iter(&contents) {
                let function_name = capture.get(1).expect("function capture should exist");
                let line = line_starts.partition_point(|offset| *offset <= function_name.start());
                let line_end = line_starts.get(line).copied().unwrap_or(contents.len());
                let source_line = Some(contents[line_starts[line - 1]..line_end].trim().to_owned());

                locations_by_test
                    .entry(capture[1].to_owned())
                    .or_insert_with(Vec::new)
                    .push(SourceLocation {
                        file: relative.clone(),
                        line: Some(line),
                        source_line,
                    });
            }
        }

        Ok(Self {
            root,
            locations_by_test,
        })
    }

    pub fn location_for(&self, test_name: &str, target_source: &Path) -> SourceLocation {
        let fallback = target_source
            .strip_prefix(&self.root)
            .unwrap_or(target_source)
            .display()
            .to_string();

        match self.locations_by_test.get(test_name).map(Vec::as_slice) {
            Some([location]) => location.clone(),
            Some(locations) => locations
                .iter()
                .find(|location| location.file == fallback)
                .cloned()
                .unwrap_or_else(|| fallback_location(fallback)),
            _ => fallback_location(fallback),
        }
    }
}

fn fallback_location(file: String) -> SourceLocation {
    SourceLocation {
        file,
        line: None,
        source_line: None,
    }
}

pub(crate) fn read_source_file(path: &Path) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_FILE_BYTES {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut contents = String::new();
    file.take(MAX_SOURCE_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .ok()?;
    (contents.len() as u64 <= MAX_SOURCE_FILE_BYTES).then_some(contents)
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut directories = vec![dir.to_owned()];
    let mut visited = 0_usize;
    while let Some(dir) = directories.pop() {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            visited += 1;
            if visited > 100_000 {
                return Ok(());
            }
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                if matches!(
                    name.as_ref(),
                    ".cargo" | ".git" | ".idea" | ".vscode" | "node_modules" | "target"
                ) {
                    continue;
                }

                directories.push(path);
                continue;
            }

            if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    Ok(())
}
