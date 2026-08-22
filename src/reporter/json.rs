use std::{
    env,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    formatting,
    runner::{PanicDetails, TestResult, TestStatus},
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

const SCHEMA_VERSION: u8 = 3;
const SOURCE_CONTEXT_RADIUS: usize = 4;

pub fn write_details_json(
    results: &[TestResult],
    total: Duration,
    output_path: &Path,
) -> Result<()> {
    let report = DetailsReport::new(results, total);
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
    working_directory: Option<String>,
    summary: SummaryDetails,
    tests: Vec<TestDetails>,
    failed: Vec<FailedTestDetails>,
}

impl DetailsReport {
    fn new(results: &[TestResult], total: Duration) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generated_at_unix_ms: unix_timestamp_ms(),
            tool: ToolMetadata {
                name: env!("CARGO_PKG_NAME"),
                version: env!("CARGO_PKG_VERSION"),
            },
            working_directory: env::current_dir()
                .ok()
                .map(|path| path.display().to_string()),
            summary: SummaryDetails::from_results(results, total),
            tests: results.iter().map(TestDetails::from).collect(),
            failed: results
                .iter()
                .filter(|result| result.status == TestStatus::Fail)
                .map(FailedTestDetails::from)
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
    successful: bool,
    total_duration_ms: u128,
    total_duration: String,
    cumulative_test_duration_ms: u128,
    average_test_duration_ms: Option<f64>,
    slowest_test: Option<TestDurationSummary>,
}

impl SummaryDetails {
    fn from_results(results: &[TestResult], total: Duration) -> Self {
        let (passed, failed, ignored) = status_counts(results);
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
            successful: failed == 0,
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

impl From<&TestResult> for TestDetails {
    fn from(result: &TestResult) -> Self {
        Self {
            id: result.id,
            status: result.status.label(),
            family: result.family.clone(),
            test: result.test.clone(),
            full_name: result.full_name.clone(),
            duration_ms: result.duration.as_millis(),
            duration_ns: result.duration.as_nanos(),
            duration: formatting::duration(result.duration),
            location: TestLocation::from_result(result),
            target: TargetDetails::from_result(result),
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
    captured_output: CapturedOutput,
}

impl From<&TestResult> for FailedTestDetails {
    fn from(result: &TestResult) -> Self {
        let failure = result.failure.as_ref();
        let stdout = failure
            .map(|failure| failure.stdout.clone())
            .unwrap_or_default();
        let stderr = failure
            .map(|failure| failure.stderr.clone())
            .unwrap_or_default();

        Self {
            id: result.id,
            status: result.status.label(),
            family: result.family.clone(),
            test: result.test.clone(),
            full_name: result.full_name.clone(),
            duration_ms: result.duration.as_millis(),
            duration_ns: result.duration.as_nanos(),
            duration: formatting::duration(result.duration),
            location: TestLocation::from_result(result),
            target: TargetDetails::from_result(result),
            rerun: RerunDetails::from_result(result),
            exit_code: failure.and_then(|failure| failure.exit_code),
            panic: failure.and_then(|failure| failure.panic.as_ref().map(PanicLocation::from)),
            assertion: failure.and_then(parse_assertion_details),
            captured_output: CapturedOutput::new(stdout, stderr),
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
    fn from_result(result: &TestResult) -> Self {
        Self {
            file: result.file.clone(),
            path: formatting::source_path(&result.file, result.line),
            line: result.line,
            function: result.test.clone(),
            source_line: result.source_line.clone(),
        }
    }
}

#[derive(Serialize)]
struct TargetDetails {
    executable: String,
    source_path: String,
}

impl TargetDetails {
    fn from_result(result: &TestResult) -> Self {
        Self {
            executable: result.executable.display().to_string(),
            source_path: result.target_source.display().to_string(),
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

impl From<&PanicDetails> for PanicLocation {
    fn from(panic: &PanicDetails) -> Self {
        Self {
            file: panic.file.clone(),
            path: formatting::source_path(&panic.file, Some(panic.line)),
            line: panic.line,
            column: panic.column,
            message: panic.message.clone(),
            source_line: source_line_at(&panic.file, panic.line),
            source_context: source_context_at(&panic.file, panic.line, SOURCE_CONTEXT_RADIUS),
        }
    }
}

#[derive(Serialize)]
struct CapturedOutput {
    stdout: String,
    stderr: String,
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
}

impl CapturedOutput {
    fn new(stdout: String, stderr: String) -> Self {
        let stdout_lines = output_lines(&stdout);
        let stderr_lines = output_lines(&stderr);
        Self {
            stdout,
            stderr,
            stdout_lines,
            stderr_lines,
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

fn output_lines(output: &str) -> Vec<String> {
    output.lines().map(str::to_owned).collect()
}
