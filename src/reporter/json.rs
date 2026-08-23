use std::{
    env,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    config::PrivacyConfig,
    formatting,
    privacy::{sanitize_path, sanitize_text},
    runner::{PanicDetails, TestResult},
};

use super::{
    DETAILS_FILE_NAME,
    context::{
        AssertionDetails, SourceContextLine, parse_assertion_details, source_context_at,
        source_line_at,
    },
    details_path,
    table::status_counts,
    write_output_file,
};

const SCHEMA_VERSION: u8 = 4;
const SOURCE_CONTEXT_RADIUS: usize = 4;

pub fn write_details_json(
    results: &[TestResult],
    total: Duration,
    output_path: &Path,
    privacy: &PrivacyConfig,
) -> Result<()> {
    let report = DetailsReport::new(results, total, privacy);
    let json = serde_json::to_string_pretty(&report).with_context(|| {
        format!(
            "failed to serialize details for {}",
            details_path(output_path).display()
        )
    })?;
    write_output_file(output_path, DETAILS_FILE_NAME, &json)
}

#[derive(Serialize)]
struct DetailsReport {
    schema_version: u8,
    generated_at_unix_ms: u128,
    tool: ToolMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
    summary: SummaryDetails,
    tests: Vec<TestDetails>,
    failed: Vec<FailedTestDetails>,
}

impl DetailsReport {
    fn new(results: &[TestResult], total: Duration, privacy: &PrivacyConfig) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generated_at_unix_ms: unix_timestamp_ms(),
            tool: ToolMetadata {
                name: env!("CARGO_PKG_NAME"),
                version: env!("CARGO_PKG_VERSION"),
            },
            working_directory: (!privacy.redact)
                .then(|| {
                    env::current_dir()
                        .ok()
                        .map(|path| path.display().to_string())
                })
                .flatten(),
            summary: SummaryDetails::from_results(results, total),
            tests: results
                .iter()
                .map(|result| TestDetails::new(result, privacy))
                .collect(),
            failed: results
                .iter()
                .filter(|result| result.status.is_failure())
                .map(|result| FailedTestDetails::new(result, privacy))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct ToolMetadata {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct SummaryDetails {
    total: usize,
    passed: usize,
    failed: usize,
    ignored: usize,
    timed_out: usize,
    successful: bool,
    total_duration_ms: u128,
    total_duration: String,
    cumulative_test_duration_ms: u128,
    average_test_duration_ms: Option<f64>,
    slowest_test: Option<TestDurationSummary>,
}

impl SummaryDetails {
    fn from_results(results: &[TestResult], total: Duration) -> Self {
        let (passed, failed, ignored, timed_out) = status_counts(results);
        let cumulative = results
            .iter()
            .map(|result| result.duration)
            .sum::<Duration>();
        let average_test_duration_ms = (!results.is_empty())
            .then(|| cumulative.as_secs_f64() * 1_000.0 / results.len() as f64);
        let slowest_test = results
            .iter()
            .max_by_key(|result| result.duration)
            .map(TestDurationSummary::from);

        Self {
            total: results.len(),
            passed,
            failed,
            ignored,
            timed_out,
            successful: failed == 0 && timed_out == 0,
            total_duration_ms: total.as_millis(),
            total_duration: formatting::duration(total),
            cumulative_test_duration_ms: cumulative.as_millis(),
            average_test_duration_ms,
            slowest_test,
        }
    }
}

#[derive(Serialize)]
struct TestDurationSummary {
    id: usize,
    full_name: String,
    duration_ms: u128,
    duration_ns: u128,
    duration: String,
}

impl From<&TestResult> for TestDurationSummary {
    fn from(result: &TestResult) -> Self {
        Self {
            id: result.id,
            full_name: result.full_name.clone(),
            duration_ms: result.duration.as_millis(),
            duration_ns: result.duration.as_nanos(),
            duration: formatting::duration(result.duration),
        }
    }
}

#[derive(Serialize)]
struct TestDetails {
    id: usize,
    status: &'static str,
    family: String,
    test: String,
    full_name: String,
    duration_ms: u128,
    duration_ns: u128,
    duration: String,
    location: TestLocation,
    target: TargetDetails,
    rerun: RerunDetails,
}

impl TestDetails {
    fn new(result: &TestResult, privacy: &PrivacyConfig) -> Self {
        Self {
            id: result.id,
            status: result.status.label(),
            family: result.family.clone(),
            test: result.test.clone(),
            full_name: result.full_name.clone(),
            duration_ms: result.duration.as_millis(),
            duration_ns: result.duration.as_nanos(),
            duration: formatting::duration(result.duration),
            location: TestLocation::from_result(result, privacy),
            target: TargetDetails::from_result(result, privacy),
            rerun: RerunDetails::from_result(result),
        }
    }
}

#[derive(Serialize)]
struct FailedTestDetails {
    id: usize,
    status: &'static str,
    family: String,
    test: String,
    full_name: String,
    duration_ms: u128,
    duration_ns: u128,
    duration: String,
    location: TestLocation,
    target: TargetDetails,
    rerun: RerunDetails,
    exit_code: Option<i32>,
    panic: Option<PanicLocation>,
    assertion: Option<AssertionDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    captured_output: Option<CapturedOutput>,
}

impl FailedTestDetails {
    fn new(result: &TestResult, privacy: &PrivacyConfig) -> Self {
        let failure = result.failure.as_ref();
        let assertion = failure
            .and_then(parse_assertion_details)
            .map(|mut assertion| {
                assertion.left = assertion
                    .left
                    .map(|value| sanitize_text(&value, privacy.redact));
                assertion.right = assertion
                    .right
                    .map(|value| sanitize_text(&value, privacy.redact));
                assertion
            });

        Self {
            id: result.id,
            status: result.status.label(),
            family: result.family.clone(),
            test: result.test.clone(),
            full_name: result.full_name.clone(),
            duration_ms: result.duration.as_millis(),
            duration_ns: result.duration.as_nanos(),
            duration: formatting::duration(result.duration),
            location: TestLocation::from_result(result, privacy),
            target: TargetDetails::from_result(result, privacy),
            rerun: RerunDetails::from_result(result),
            exit_code: failure.and_then(|failure| failure.exit_code),
            panic: failure.and_then(|failure| {
                failure
                    .panic
                    .as_ref()
                    .map(|panic| PanicLocation::new(panic, privacy))
            }),
            assertion,
            captured_output: privacy.include_captured_output.then(|| {
                failure.map_or_else(CapturedOutput::empty, |failure| {
                    CapturedOutput::new(failure, privacy)
                })
            }),
        }
    }
}

#[derive(Serialize)]
struct TestLocation {
    file: String,
    path: String,
    line: Option<usize>,
    function: String,
    source_line: Option<String>,
}

impl TestLocation {
    fn from_result(result: &TestResult, privacy: &PrivacyConfig) -> Self {
        Self {
            file: sanitize_path(&result.file, privacy.redact),
            path: formatting::source_path(
                &sanitize_path(&result.file, privacy.redact),
                result.line,
            ),
            line: result.line,
            function: result.test.clone(),
            source_line: privacy
                .include_source_context
                .then(|| {
                    result
                        .source_line
                        .as_ref()
                        .map(|line| sanitize_text(line, privacy.redact))
                })
                .flatten(),
        }
    }
}

#[derive(Serialize)]
struct TargetDetails {
    executable: String,
    source_path: String,
}

impl TargetDetails {
    fn from_result(result: &TestResult, privacy: &PrivacyConfig) -> Self {
        Self {
            executable: sanitize_path(&result.executable.display().to_string(), privacy.redact),
            source_path: sanitize_path(&result.target_source.display().to_string(), privacy.redact),
        }
    }
}

#[derive(Serialize)]
struct PanicLocation {
    file: String,
    path: String,
    line: usize,
    column: Option<usize>,
    message: Option<String>,
    source_line: Option<String>,
    source_context: Vec<SourceContextLine>,
}

impl PanicLocation {
    fn new(panic: &PanicDetails, privacy: &PrivacyConfig) -> Self {
        let file = sanitize_path(&panic.file, privacy.redact);
        Self {
            file: file.clone(),
            path: formatting::source_path(&file, Some(panic.line)),
            line: panic.line,
            column: panic.column,
            message: panic
                .message
                .as_ref()
                .map(|message| sanitize_text(message, privacy.redact)),
            source_line: privacy
                .include_source_context
                .then(|| {
                    source_line_at(&panic.file, panic.line)
                        .map(|line| sanitize_text(&line, privacy.redact))
                })
                .flatten(),
            source_context: if privacy.include_source_context {
                source_context_at(&panic.file, panic.line, SOURCE_CONTEXT_RADIUS)
                    .into_iter()
                    .map(|mut line| {
                        line.text = sanitize_text(&line.text, privacy.redact);
                        line
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }
    }
}

#[derive(Serialize)]
struct CapturedOutput {
    stdout: String,
    stderr: String,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

impl CapturedOutput {
    fn new(failure: &crate::runner::FailureOutput, privacy: &PrivacyConfig) -> Self {
        Self {
            stdout: sanitize_text(&failure.stdout, privacy.redact),
            stderr: sanitize_text(&failure.stderr, privacy.redact),
            stdout_truncated: failure.stdout_truncated,
            stderr_truncated: failure.stderr_truncated,
        }
    }

    fn empty() -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }
}

#[derive(Serialize)]
struct RerunDetails {
    cargo_tester: String,
    cargo_test: String,
}

impl RerunDetails {
    fn from_result(result: &TestResult) -> Self {
        Self {
            cargo_tester: format!("cargo tester {}", result.full_name),
            cargo_test: format!("cargo test {} -- --exact", result.full_name),
        }
    }
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
