use std::{
    collections::HashSet,
    env,
    path::PathBuf,
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::source::SourceIndex;

#[derive(Debug, Clone)]
pub struct TestCase {
    pub full_name: String,
    pub executable: PathBuf,
    pub source_path: PathBuf,
}

#[derive(Debug)]
pub struct TestResult {
    pub id: usize,
    pub full_name: String,
    pub file: String,
    pub family: String,
    pub test: String,
    pub line: Option<usize>,
    pub source_line: Option<String>,
    pub status: TestStatus,
    pub duration: Duration,
    pub executable: PathBuf,
    pub target_source: PathBuf,
    pub failure: Option<FailureOutput>,
}

#[derive(Debug)]
pub struct FailureOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub panic: Option<PanicDetails>,
}

#[derive(Debug)]
pub struct PanicDetails {
    pub file: String,
    pub line: usize,
    pub column: Option<usize>,
    pub message: Option<String>,
}

impl PanicDetails {
    pub fn parse(output: &str) -> Option<Self> {
        parse_panic_details(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Ignored,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Ignored => "IGNORED",
        }
    }
}

pub struct DiscoveryOptions<'a> {
    pub filter: Option<&'a str>,
    pub cargo_args: &'a [String],
}

pub struct ExecutionOptions<'a> {
    pub harness_args: &'a [String],
    pub jobs: usize,
    pub sequential_tests: &'a HashSet<String>,
}

pub fn discover_tests(options: DiscoveryOptions<'_>) -> Result<Vec<TestCase>> {
    let mut command = cargo_command();
    command.arg("test");
    command.args(options.cargo_args);
    let output = command
        .args(["--no-run", "--message-format=json"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .context("failed to compile Rust tests")?;

    if !output.status.success() {
        return Err(cargo_error("failed to compile Rust tests", &output, true));
    }

    let artifacts = test_artifacts(&output.stdout)?;
    let mut tests = Vec::new();

    for artifact in artifacts {
        let output = Command::new(&artifact.executable)
            .args(["--list", "--format=terse"])
            .output()
            .with_context(|| {
                format!(
                    "failed to list tests from {}",
                    artifact.executable.display()
                )
            })?;

        if !output.status.success() {
            return Err(cargo_error(
                &format!(
                    "failed to list tests from {}",
                    artifact.executable.display()
                ),
                &output,
                false,
            ));
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Some(test_name) = line.trim().strip_suffix(": test") else {
                continue;
            };

            if options
                .filter
                .is_none_or(|filter| test_name.contains(filter))
            {
                tests.push(TestCase {
                    full_name: test_name.to_owned(),
                    executable: artifact.executable.clone(),
                    source_path: artifact.source_path.clone(),
                });
            }
        }
    }

    tests.sort_by(|left, right| {
        left.full_name
            .cmp(&right.full_name)
            .then_with(|| left.source_path.cmp(&right.source_path))
            .then_with(|| left.executable.cmp(&right.executable))
    });
    Ok(tests)
}

fn cargo_command() -> Command {
    env::var_os("CARGO").map_or_else(|| Command::new("cargo"), Command::new)
}

pub fn run_single_test(
    id: usize,
    test_case: &TestCase,
    sources: &SourceIndex,
) -> Result<TestResult> {
    run_single_test_with_args(id, test_case, sources, &[])
}

pub fn run_tests(
    tests: &[TestCase],
    sources: &SourceIndex,
    options: ExecutionOptions<'_>,
) -> Result<Vec<TestResult>> {
    let (parallel, sequential): (Vec<_>, Vec<_>) = tests
        .iter()
        .enumerate()
        .partition(|(_, test)| !options.sequential_tests.contains(&test.full_name));
    let mut results = run_parallel_tests(
        &parallel,
        sources,
        options.harness_args,
        options.jobs.max(1),
    )?;

    for (index, test) in sequential {
        results.push(run_single_test_with_args(
            index + 1,
            test,
            sources,
            options.harness_args,
        )?);
    }

    results.sort_by_key(|result| result.id);
    Ok(results)
}

fn run_parallel_tests(
    tests: &[(usize, &TestCase)],
    sources: &SourceIndex,
    harness_args: &[String],
    jobs: usize,
) -> Result<Vec<TestResult>> {
    if tests.is_empty() {
        return Ok(Vec::new());
    }

    let next = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(Vec::with_capacity(tests.len())));
    let worker_count = jobs.min(tests.len());

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let next = Arc::clone(&next);
            let results = Arc::clone(&results);
            scope.spawn(move || {
                loop {
                    let work_index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((index, test)) = tests.get(work_index) else {
                        break;
                    };
                    let result = run_single_test_with_args(index + 1, test, sources, harness_args);
                    results
                        .lock()
                        .expect("test result lock should not be poisoned")
                        .push(result);
                }
            });
        }
    });

    Arc::into_inner(results)
        .expect("all test workers should be complete")
        .into_inner()
        .expect("test result lock should not be poisoned")
        .into_iter()
        .collect()
}

fn run_single_test_with_args(
    id: usize,
    test_case: &TestCase,
    sources: &SourceIndex,
    harness_args: &[String],
) -> Result<TestResult> {
    let started = Instant::now();
    let mut command = Command::new(&test_case.executable);
    command.args([
        &test_case.full_name,
        "--exact",
        "--format=terse",
        "--color=never",
    ]);
    let output = command
        .args(harness_args)
        .output()
        .with_context(|| format!("failed to run test {}", test_case.full_name))?;
    let duration = started.elapsed();
    let combined = combined_output(&output);

    let status = if output.status.success() {
        let passed = summary_count(&combined, "passed");
        let ignored = summary_count(&combined, "ignored");

        if ignored == 1 && passed == 0 {
            TestStatus::Ignored
        } else if passed == 1 && ignored == 0 {
            TestStatus::Pass
        } else {
            bail!("test {} produced no usable result", test_case.full_name);
        }
    } else {
        TestStatus::Fail
    };
    let failure = if status == TestStatus::Fail {
        Some(FailureOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
            panic: PanicDetails::parse(&combined),
        })
    } else {
        None
    };

    let test = test_case
        .full_name
        .rsplit("::")
        .next()
        .unwrap_or(&test_case.full_name)
        .to_owned();
    let family = test_case
        .full_name
        .rsplit_once("::")
        .map(|(family, _)| family.to_owned())
        .unwrap_or_else(|| "-".to_owned());
    let location = sources.location_for(&test, &test_case.source_path);

    Ok(TestResult {
        id,
        full_name: test_case.full_name.clone(),
        file: location.file,
        family,
        test,
        line: location.line,
        source_line: location.source_line,
        status,
        duration,
        executable: test_case.executable.clone(),
        target_source: test_case.source_path.clone(),
        failure,
    })
}

pub fn has_failures(results: &[TestResult]) -> bool {
    results
        .iter()
        .any(|result| result.status == TestStatus::Fail)
}

fn summary_count(output: &str, label: &str) -> u32 {
    output
        .lines()
        .filter(|line| line.contains("test result:"))
        .flat_map(|line| line.split(';'))
        .find_map(|part| {
            let mut previous = None;
            for word in part.split_whitespace() {
                if word == label {
                    return previous.and_then(|value: &str| value.parse().ok());
                }
                previous = Some(word);
            }
            None
        })
        .unwrap_or(0)
}

#[derive(Debug)]
struct TestArtifact {
    executable: PathBuf,
    source_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoMessage {
    reason: String,
    executable: Option<PathBuf>,
    profile: Option<CargoProfile>,
    target: Option<CargoTarget>,
    message: Option<CargoDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct CargoProfile {
    test: bool,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    src_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoDiagnostic {
    rendered: Option<String>,
}

fn test_artifacts(stdout: &[u8]) -> Result<Vec<TestArtifact>> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut seen = HashSet::new();
    let mut artifacts = Vec::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let message: CargoMessage =
            serde_json::from_str(line).context("Cargo returned an invalid JSON build message")?;

        if message.reason != "compiler-artifact"
            || !message.profile.is_some_and(|profile| profile.test)
        {
            continue;
        }

        let (Some(executable), Some(target)) = (message.executable, message.target) else {
            continue;
        };

        if seen.insert(executable.clone()) {
            artifacts.push(TestArtifact {
                executable,
                source_path: target.src_path,
            });
        }
    }

    Ok(artifacts)
}

fn combined_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{stdout}{stderr}")
}

fn parse_panic_details(output: &str) -> Option<PanicDetails> {
    let mut lines = output.lines();

    while let Some(line) = lines.next() {
        let Some((_, location)) = line.split_once("panicked at ") else {
            continue;
        };
        let location = location.trim();

        if let Some(legacy) = location.strip_prefix('\'') {
            let (message, location) = legacy.rsplit_once("', ")?;
            let (file, panic_line, column) = parse_rust_location(location.trim_end_matches(':'))?;
            return Some(PanicDetails {
                file,
                line: panic_line,
                column,
                message: Some(message.to_owned()),
            });
        }

        let location = location.trim_end_matches(':');
        let (file, panic_line, column) = parse_rust_location(location)?;
        let message = lines
            .by_ref()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("note:"))
            .map(str::to_owned);

        return Some(PanicDetails {
            file,
            line: panic_line,
            column,
            message,
        });
    }

    None
}

fn parse_rust_location(location: &str) -> Option<(String, usize, Option<usize>)> {
    let parts: Vec<&str> = location.rsplitn(3, ':').collect();

    if parts.len() == 3 {
        if let (Ok(column), Ok(line)) = (parts[0].parse(), parts[1].parse()) {
            return Some((parts[2].to_owned(), line, Some(column)));
        }
    }

    let (file, line) = location.rsplit_once(':')?;
    Some((file.to_owned(), line.parse().ok()?, None))
}

fn cargo_error(message: &str, output: &Output, json_stdout: bool) -> anyhow::Error {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let details = if json_stdout {
        stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
            .filter_map(|message| message.message?.rendered)
            .collect::<Vec<_>>()
            .join("")
    } else {
        stdout.into_owned()
    };
    let details = format!("{details}{stderr}");

    if details.trim().is_empty() {
        anyhow!(message.to_owned())
    } else {
        anyhow!("{message}\n\n{}", details.trim())
    }
}
