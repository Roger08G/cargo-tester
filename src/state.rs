use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::runner::{TestResult, TestStatus};

const LAST_RUN_FILE_NAME: &str = "last-run.json";
const SCHEMA_VERSION: u8 = 2;

#[derive(Debug)]
pub struct PreviousRun {
    pub failed_tests: HashSet<String>,
    pub cargo_args: Vec<String>,
    pub harness_args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LastRunState {
    schema_version: u8,
    generated_at_unix_ms: u128,
    failed_tests: Vec<String>,
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
    if !matches!(state.schema_version, 1 | SCHEMA_VERSION) {
        bail!(
            "unsupported {} schema version {}",
            path.display(),
            state.schema_version
        );
    }

    Ok(Some(PreviousRun {
        failed_tests: state.failed_tests.into_iter().collect(),
        cargo_args: state.cargo_args,
        harness_args: state.harness_args,
    }))
}

pub fn load_failed_tests(output_path: &Path) -> Result<Option<HashSet<String>>> {
    Ok(load_last_run(output_path)?.map(|state| state.failed_tests))
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
    let state = LastRunState {
        schema_version: SCHEMA_VERSION,
        generated_at_unix_ms: unix_timestamp_ms(),
        failed_tests: results
            .iter()
            .filter(|result| result.status == TestStatus::Fail)
            .map(|result| result.full_name.clone())
            .collect(),
        cargo_args: cargo_args.to_vec(),
        harness_args: harness_args.to_vec(),
    };
    let json = serde_json::to_string_pretty(&state).context("failed to serialize last run")?;

    fs::create_dir_all(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let path = output_path.join(LAST_RUN_FILE_NAME);
    fs::write(&path, format!("{}\n", json.trim_end()))
        .with_context(|| format!("failed to write {}", path.display()))
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
