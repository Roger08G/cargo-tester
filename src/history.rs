use std::{
    collections::HashMap,
    env, fs,
    io::ErrorKind,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    config::HistoryConfig,
    runner::{TestResult, TestStatus},
};

const HISTORY_FILE_NAME: &str = "history.json";
const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TestHistory {
    pub id: usize,
    pub full_name: String,
    pub status: String,
    pub duration_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RunHistory {
    pub generated_at_unix_ms: u128,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub total_duration_ms: u64,
    pub tests: Vec<TestHistory>,
}

#[derive(Debug)]
pub struct PerformanceRegression {
    pub full_name: String,
    pub previous_duration: Duration,
    pub current_duration: Duration,
    pub increase_percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HistoryFile {
    schema_version: u8,
    runs: Vec<RunHistory>,
}

impl Default for HistoryFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            runs: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct History {
    file: HistoryFile,
}

impl History {
    pub fn load(output_path: &Path) -> Result<Self> {
        let path = output_path.join(HISTORY_FILE_NAME);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let file: HistoryFile = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if file.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported {} schema version {}",
                path.display(),
                file.schema_version
            );
        }
        Ok(Self { file })
    }

    pub fn runs(&self) -> &[RunHistory] {
        &self.file.runs
    }

    pub fn regressions(
        &self,
        results: &[TestResult],
        config: &HistoryConfig,
    ) -> Vec<PerformanceRegression> {
        let Some(previous) = self.file.runs.last() else {
            return Vec::new();
        };
        let previous_tests = previous
            .tests
            .iter()
            .filter(|test| test.status == TestStatus::Pass.label())
            .map(|test| (test.full_name.as_str(), test.duration_ms))
            .collect::<HashMap<_, _>>();

        let mut regressions = results
            .iter()
            .filter(|result| result.status == TestStatus::Pass)
            .filter_map(|result| {
                let previous_ms = *previous_tests.get(result.full_name.as_str())?;
                let previous_duration = Duration::from_millis(previous_ms);
                if previous_duration < config.minimum_test_duration()
                    || result.duration <= previous_duration
                    || previous_ms == 0
                {
                    return None;
                }

                let increase_percent =
                    (result.duration.as_secs_f64() / previous_duration.as_secs_f64() - 1.0) * 100.0;
                (increase_percent >= config.regression_threshold_percent).then(|| {
                    PerformanceRegression {
                        full_name: result.full_name.clone(),
                        previous_duration,
                        current_duration: result.duration,
                        increase_percent,
                    }
                })
            })
            .collect::<Vec<_>>();
        regressions.sort_by(|left, right| {
            right
                .increase_percent
                .total_cmp(&left.increase_percent)
                .then_with(|| left.full_name.cmp(&right.full_name))
        });
        regressions
    }

    pub fn record(
        &mut self,
        results: &[TestResult],
        total: Duration,
        config: &HistoryConfig,
        output_path: &Path,
    ) -> Result<()> {
        let (passed, failed, ignored) = status_counts(results);
        self.file.runs.push(RunHistory {
            generated_at_unix_ms: unix_timestamp_ms(),
            working_directory: env::current_dir()
                .ok()
                .map(|path| path.display().to_string()),
            arguments: env::args().skip(1).collect(),
            total: results.len(),
            passed,
            failed,
            ignored,
            total_duration_ms: duration_ms(total),
            tests: results
                .iter()
                .map(|result| TestHistory {
                    id: result.id,
                    full_name: result.full_name.clone(),
                    status: result.status.label().to_owned(),
                    duration_ms: duration_ms(result.duration),
                })
                .collect(),
        });

        let excess = self.file.runs.len().saturating_sub(config.max_runs);
        if excess > 0 {
            self.file.runs.drain(..excess);
        }
        self.write(output_path)
    }

    fn write(&self, output_path: &Path) -> Result<()> {
        fs::create_dir_all(output_path)
            .with_context(|| format!("failed to create {}", output_path.display()))?;
        let path = output_path.join(HISTORY_FILE_NAME);
        let json =
            serde_json::to_string_pretty(&self.file).context("failed to serialize test history")?;
        fs::write(&path, format!("{}\n", json.trim_end()))
            .with_context(|| format!("failed to write {}", path.display()))
    }
}

fn status_counts(results: &[TestResult]) -> (usize, usize, usize) {
    results.iter().fold(
        (0, 0, 0),
        |(passed, failed, ignored), result| match result.status {
            TestStatus::Pass => (passed + 1, failed, ignored),
            TestStatus::Fail => (passed, failed + 1, ignored),
            TestStatus::Ignored => (passed, failed, ignored + 1),
        },
    )
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
