use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, Result};
use regex::Regex;

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

        for path in rust_files {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();

            for capture in TEST_FUNCTION.captures_iter(&contents) {
                let function_name = capture.get(1).expect("function capture should exist");
                let line = line_number(&contents, function_name.start());
                let source_line = contents
                    .lines()
                    .nth(line.saturating_sub(1))
                    .map(|line| line.trim().to_owned());

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

fn line_number(contents: &str, byte_index: usize) -> usize {
    contents[..byte_index]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
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

            collect_rust_files(&path, files)?;
            continue;
        }

        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }

    Ok(())
}
