use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    config::PrivacyConfig,
    persistence::atomic_write,
    privacy::sanitize_path,
    runner::{TestCase, TestResult},
};

const LAST_RUN_FILE_NAME: &str = "last-run.json";
const SCHEMA_VERSION: u8 = 3;

#[derive(Debug)]
pub struct PreviousRun {
    failed_tests: Vec<StoredFailedTest>,
    redacted: bool,
    pub cargo_args: Vec<String>,
    pub harness_args: Vec<String>,
}

impl PreviousRun {
    pub fn failed_tests(&self) -> HashSet<String> {
        self.failed_tests
            .iter()
            .map(|test| test.full_name().to_owned())
            .collect()
    }

    pub fn has_failures(&self) -> bool {
        !self.failed_tests.is_empty()
    }

    pub fn matches(&self, test: &TestCase) -> bool {
        let target_source = target_key(&test.source_path);
        let target_source = sanitize_path(&target_source, self.redacted);
        self.failed_tests.iter().any(|failed| match failed {
            StoredFailedTest::Legacy(full_name) => full_name == &test.full_name,
            StoredFailedTest::Detailed {
                full_name,
                target_source: failed_target,
            } => full_name == &test.full_name && failed_target == &target_source,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum StoredFailedTest {
    Detailed {
        full_name: String,
        target_source: String,
    },
    Legacy(String),
}

impl StoredFailedTest {
    fn full_name(&self) -> &str {
        match self {
            Self::Detailed { full_name, .. } | Self::Legacy(full_name) => full_name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LastRunState {
    schema_version: u8,
    generated_at_unix_ms: u128,
    failed_tests: Vec<StoredFailedTest>,
    #[serde(default)]
    redacted: bool,
    #[serde(default)]
    cargo_args: Vec<String>,
    #[serde(default)]
    harness_args: Vec<String>,
}

pub fn load_last_run(output_path: &Path) -> Result<Option<PreviousRun>> {
    let path = output_path.join(LAST_RUN_FILE_NAME);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let state: LastRunState = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if !matches!(state.schema_version, 1 | 2 | SCHEMA_VERSION) {
        bail!(
            "unsupported {} schema version {}",
            path.display(),
            state.schema_version
        );
    }

    Ok(Some(PreviousRun {
        failed_tests: state.failed_tests,
        redacted: state.redacted,
        cargo_args: state.cargo_args,
        harness_args: state.harness_args,
    }))
}

pub fn load_failed_tests(output_path: &Path) -> Result<Option<HashSet<String>>> {
    Ok(load_last_run(output_path)?.map(|state| state.failed_tests()))
}

pub fn write_last_run(results: &[TestResult], output_path: &Path) -> Result<()> {
    write_last_run_with_options(results, output_path, &[], &[])
}

pub fn write_last_run_with_options(
    results: &[TestResult],
    output_path: &Path,
    cargo_args: &[String],
    harness_args: &[String],
) -> Result<()> {
    write_last_run_with_privacy(
        results,
        output_path,
        cargo_args,
        harness_args,
        &PrivacyConfig::default(),
    )
}

pub(crate) fn write_last_run_with_privacy(
    results: &[TestResult],
    output_path: &Path,
    cargo_args: &[String],
    harness_args: &[String],
    privacy: &PrivacyConfig,
) -> Result<()> {
    let state = LastRunState {
        schema_version: SCHEMA_VERSION,
        generated_at_unix_ms: unix_timestamp_ms(),
        failed_tests: results
            .iter()
            .filter(|result| result.status.is_failure())
            .map(|result| StoredFailedTest::Detailed {
                full_name: result.full_name.clone(),
                target_source: sanitize_path(&target_key(&result.target_source), privacy.redact),
            })
            .collect(),
        redacted: privacy.redact,
        cargo_args: if privacy.redact {
            Vec::new()
        } else {
            cargo_args.to_vec()
        },
        harness_args: if privacy.redact {
            Vec::new()
        } else {
            harness_args.to_vec()
        },
    };
    let json = serde_json::to_string_pretty(&state).context("failed to serialize last run")?;

    let path = output_path.join(LAST_RUN_FILE_NAME);
    atomic_write(&path, format!("{}\n", json.trim_end()).as_bytes())
}

fn target_key(path: &Path) -> String {
    let relative = std::env::current_dir()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok().map(PathBuf::from))
        .unwrap_or_else(|| path.to_owned());
    relative.display().to_string().replace('\\', "/")
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
