use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static E2E_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn failing_test_produces_sanitized_ci_details() {
    let _guard = e2e_guard();
    let fixture = Fixture::package("privacy_failure");
    fixture.write(
        "src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn fails_with_sensitive_output() {
        println!("password=ci-super-secret-value");
        panic!("token=ghp_abcdefghijklmnopqrstuvwxyz123456");
    }
}
"#,
    );
    fixture.write("tester.toml", &standard_config("5s", 262_144));

    let local_output = run_tester(&fixture.root, &["--details"]);
    assert!(!local_output.status.success());
    let local_history: Value = serde_json::from_str(
        &fs::read_to_string(fixture.root.join(".cargo/tester-output/history.json"))
            .expect("local history should exist"),
    )
    .expect("local history should be valid JSON");
    let stored_working_directory = local_history["runs"][0]["working_directory"]
        .as_str()
        .expect("local working directory should be text");
    assert_eq!(
        fs::canonicalize(stored_working_directory).expect("stored working directory should exist"),
        fs::canonicalize(&fixture.root).expect("fixture root should exist")
    );

    let output = run_tester(&fixture.root, &["--ci"]);
    assert!(!output.status.success(), "a failing test must fail the run");
    let details = fixture.details();
    let serialized = details.to_string();
    let output_dir = fixture.root.join(".cargo/tester-output");

    assert_eq!(details["schema_version"], 4);
    assert_eq!(details["summary"]["failed"], 1);
    assert!(details.get("working_directory").is_none());
    assert!(!json_contains_text(
        &details,
        &fixture.root.display().to_string()
    ));
    assert!(!json_contains_text(&details, stored_working_directory));
    assert!(!serialized.contains("ci-super-secret-value"));
    assert!(!serialized.contains("ghp_"));
    assert!(details["failed"][0].get("captured_output").is_none());
    assert_eq!(
        details["failed"][0]["panic"]["source_context"],
        Value::Array(Vec::new())
    );
    for name in [
        "summary.txt",
        "details.json",
        "history.json",
        "last-run.json",
    ] {
        let artifact = fs::read_to_string(output_dir.join(name)).expect("CI artifact should exist");
        assert!(!artifact.contains(&fixture.root.display().to_string()));
        assert!(!artifact.contains(stored_working_directory));
        assert!(!artifact.contains("ci-super-secret-value"));
        assert!(!artifact.contains("ghp_"));
    }
    let history: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("history.json")).expect("history should exist"),
    )
    .expect("history should be valid JSON");
    assert!(history["runs"][0]["working_directory"].is_null());
    assert_eq!(history["runs"][0]["arguments"], Value::Array(Vec::new()));
    assert!(!json_contains_text(
        &history,
        &fixture.root.display().to_string()
    ));
    let last_run: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("last-run.json")).expect("last run should exist"),
    )
    .expect("last run should be valid JSON");
    assert_eq!(last_run["cargo_args"], Value::Array(Vec::new()));
    assert_eq!(last_run["harness_args"], Value::Array(Vec::new()));
    assert!(!json_contains_text(
        &last_run,
        &fixture.root.display().to_string()
    ));
}

#[test]
fn bounds_massive_output_and_terminates_hanging_tests() {
    let _guard = e2e_guard();
    let fixture = Fixture::package("bounded_timeout");
    fixture.write(
        "src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn emits_massive_output() {
        print!("{}", "x".repeat(2_000_000));
        panic!("intentional output failure");
    }

    #[test]
    fn hangs() {
        #[cfg(windows)]
        let _child = std::process::Command::new("cmd")
            .args(["/C", "ping -n 11 127.0.0.1 >NUL"])
            .spawn()
            .expect("child process should start");
        #[cfg(unix)]
        let _child = std::process::Command::new("sh")
            .args(["-c", "sleep 10"])
            .spawn()
            .expect("child process should start");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
"#,
    );
    fixture.write("tester.toml", &standard_config("300ms", 32_768));

    let flood = run_tester(&fixture.root, &["--details", "emits_massive_output"]);
    assert!(!flood.status.success());
    let details = fixture.details();
    let captured = &details["failed"][0]["captured_output"];
    assert_eq!(captured["stdout_truncated"], true);
    assert!(
        captured["stdout"]
            .as_str()
            .expect("stdout must be text")
            .len()
            <= 32_768,
        "retained stdout exceeded the configured byte limit"
    );

    let started = Instant::now();
    let timeout = run_tester(&fixture.root, &["--details", "hangs"]);
    let elapsed = started.elapsed();
    assert!(!timeout.status.success());
    assert!(
        elapsed < Duration::from_secs(8),
        "timed-out process was not terminated promptly: {elapsed:?}"
    );
    let timeout_stdout = String::from_utf8_lossy(&timeout.stdout);
    assert!(timeout_stdout.contains("TIMEOUT"));
    let details = fixture.details();
    assert_eq!(details["summary"]["timed_out"], 1);
    assert_eq!(details["failed"][0]["status"], "TIMEOUT");
}

#[test]
fn supports_ignored_selection_and_duplicate_names_in_workspaces() {
    let _guard = e2e_guard();

    let ignored = Fixture::package("ignored_selection");
    ignored.write(
        "src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "covered explicitly"]
    fn ignored_case() {}
}
"#,
    );
    ignored.write("tester.toml", &standard_config("5s", 262_144));
    let output = run_tester(&ignored.root, &["--ci", "--", "--ignored"]);
    assert_success(&output, "ignored-only selection");
    assert_eq!(ignored.details()["summary"]["passed"], 1);

    let workspace = Fixture::workspace("duplicate_workspace", &["member-a", "member-b"]);
    workspace.write("tester.toml", &standard_config("5s", 262_144));
    workspace.write(
        "member-a/src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn duplicate_name() {}
}
"#,
    );
    workspace.write(
        "member-b/src/lib.rs",
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn duplicate_name() { panic!("only member-b fails"); }
}
"#,
    );

    let output = run_tester(&workspace.root, &["--ci", "--workspace"]);
    assert!(!output.status.success());
    let details = workspace.details();
    assert_eq!(details["summary"]["total"], 2);
    assert_eq!(details["summary"]["passed"], 1);
    assert_eq!(details["summary"]["failed"], 1);

    let output = run_tester(&workspace.root, &["--ci", "--last-failed", "--workspace"]);
    assert!(!output.status.success());
    let details = workspace.details();
    assert_eq!(details["summary"]["total"], 1);
    assert!(
        details["tests"][0]["target"]["source_path"]
            .as_str()
            .expect("source path must be text")
            .contains("member-b/src/lib.rs")
    );
}

#[test]
fn reports_compile_errors_and_custom_harnesses_explicitly() {
    let _guard = e2e_guard();

    let compile_error = Fixture::package("compile_error");
    compile_error.write(
        "src/lib.rs",
        "compile_error!(\"password=compile-secret-value\");\n",
    );
    compile_error.write("tester.toml", &standard_config("5s", 262_144));
    let output = run_tester(&compile_error.root, &["--ci"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to compile Rust tests"),
        "compile failure was not reported clearly: {}",
        output_text(&output)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("compile-secret-value"));
    assert!(
        !String::from_utf8_lossy(&output.stderr)
            .contains(&compile_error.root.display().to_string())
    );

    let custom = Fixture::package("custom_harness");
    custom.write(
        "Cargo.toml",
        r#"
[package]
name = "custom-harness-fixture"
version = "0.0.0"
edition = "2024"

[[test]]
name = "custom"
path = "tests/custom.rs"
harness = false
"#,
    );
    custom.write("src/lib.rs", "pub fn library() {}\n");
    custom.write(
        "tests/custom.rs",
        "fn main() { println!(\"custom harness protocol\"); }\n",
    );
    custom.write("tester.toml", &standard_config("5s", 262_144));
    let output = run_tester(&custom.root, &["--ci", "--test", "custom"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("standard libtest harness only"), "{stderr}");

    custom.write("tests/custom.rs", "fn main() {}\n");
    let output = run_tester(&custom.root, &["--ci", "--test", "custom"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("standard libtest harness only"), "{stderr}");

    let hanging = Fixture::package("hanging_discovery");
    hanging.write(
        "Cargo.toml",
        r#"
[package]
name = "hanging-discovery-fixture"
version = "0.0.0"
edition = "2024"

[[test]]
name = "hanging"
path = "tests/hanging.rs"
harness = false
"#,
    );
    hanging.write("src/lib.rs", "pub fn library() {}\n");
    hanging.write(
        "tests/hanging.rs",
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }\n",
    );
    hanging.write(
        "tester.toml",
        &standard_config("5s", 262_144).replace(
            "discovery-timeout = \"2m\"",
            "discovery-timeout = \"300ms\"",
        ),
    );
    assert_success(&compile_fixture(&hanging.root), "hanging discovery warm-up");
    let started = Instant::now();
    let output = run_tester(&hanging.root, &["--ci", "--test", "hanging"]);
    let elapsed = started.elapsed();
    assert!(!output.status.success());
    assert!(elapsed < Duration::from_secs(8), "{elapsed:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("exceeded the configured timeout"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn concurrent_runs_keep_history_valid_and_complete() {
    let _guard = e2e_guard();
    let fixture = Fixture::package("concurrent_history");
    fixture.write(
        "src/lib.rs",
        "#[cfg(test)] mod tests { #[test] fn passes() {} }\n",
    );
    fixture.write("tester.toml", &standard_config("5s", 262_144));
    assert_success(&compile_fixture(&fixture.root), "fixture warm-up");

    let mut first = tester_command(&fixture.root);
    first
        .arg("--ci")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut second = tester_command(&fixture.root);
    second
        .arg("--ci")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let first = first.spawn().expect("first concurrent run should start");
    let second = second.spawn().expect("second concurrent run should start");
    let first = first.wait_with_output().expect("first run should finish");
    let second = second.wait_with_output().expect("second run should finish");
    assert_success(&first, "first concurrent run");
    assert_success(&second, "second concurrent run");

    let history: Value = serde_json::from_str(
        &fs::read_to_string(fixture.root.join(".cargo/tester-output/history.json"))
            .expect("history should exist"),
    )
    .expect("concurrent history must remain valid JSON");
    assert_eq!(
        history["runs"]
            .as_array()
            .expect("runs must be an array")
            .len(),
        2
    );
}

#[test]
#[ignore = "manual execution-model benchmark; run with --ignored --nocapture"]
fn benchmark_execution_models() {
    let _guard = e2e_guard();
    let fixture = Fixture::package("execution_benchmark");
    let mut source = String::from("#[cfg(test)] mod tests {\n");
    for index in 0..24 {
        source.push_str(&format!(
            "#[test] fn case_{index}() {{ std::hint::black_box({index}); }}\n"
        ));
    }
    source.push_str("}\n");
    fixture.write("src/lib.rs", &source);
    fixture.write("tester.toml", &standard_config("5s", 262_144));
    assert_success(&compile_fixture(&fixture.root), "benchmark warm-up");
    assert_success(
        &cargo_command(&fixture.root)
            .args(["test", "--lib", "--quiet"])
            .output()
            .unwrap(),
        "native benchmark warm-up",
    );
    assert_success(
        &run_tester(&fixture.root, &["--ci", "--lib"]),
        "cargo-tester benchmark warm-up",
    );

    let mut native_samples = Vec::with_capacity(5);
    let mut isolated_samples = Vec::with_capacity(5);
    for sample in 0..5 {
        if sample % 2 == 0 {
            native_samples.push(measure_native(&fixture.root));
            isolated_samples.push(measure_isolated(&fixture.root));
        } else {
            isolated_samples.push(measure_isolated(&fixture.root));
            native_samples.push(measure_native(&fixture.root));
        }
    }
    native_samples.sort_unstable();
    isolated_samples.sort_unstable();
    let native_median = native_samples[2];
    let isolated_median = isolated_samples[2];

    println!(
        "execution-model benchmark: tests=24 samples=5 cargo_test_median_ms={} cargo_tester_median_ms={} ratio={:.2}",
        native_median.as_millis(),
        isolated_median.as_millis(),
        isolated_median.as_secs_f64() / native_median.as_secs_f64().max(f64::EPSILON)
    );
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn package(name: &str) -> Self {
        let fixture = Self::new(name);
        fixture.write(
            "Cargo.toml",
            &format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"),
        );
        fixture
    }

    fn workspace(name: &str, members: &[&str]) -> Self {
        let fixture = Self::new(name);
        let member_list = members
            .iter()
            .map(|member| format!("\"{member}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fixture.write(
            "Cargo.toml",
            &format!("[workspace]\nresolver = \"2\"\nmembers = [{member_list}]\n"),
        );
        for member in members {
            fixture.write(
                &format!("{member}/Cargo.toml"),
                &format!(
                    "[package]\nname = \"{member}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"
                ),
            );
        }
        fixture
    }

    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must follow the Unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "cargo_tester_e2e_{name}_{}_{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&root).expect("fixture root should be created");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture directory should be created");
        }
        fs::write(path, contents).expect("fixture file should be written");
    }

    fn details(&self) -> Value {
        serde_json::from_str(
            &fs::read_to_string(self.root.join(".cargo/tester-output/details.json"))
                .expect("details.json should exist"),
        )
        .expect("details.json should be valid JSON")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn standard_config(test_timeout: &str, max_output_bytes: usize) -> String {
    format!(
        r#"
[output]
color = false
unicode = false
emoji = false
output-path = ".cargo/tester-output/"

[execution]
jobs = 2
test-timeout = "{test_timeout}"
discovery-timeout = "2m"
max-discovery-output-bytes = 2097152
max-output-bytes = {max_output_bytes}

[history]
enabled = true
max-runs = 20
show-runs-history = 20
"#
    )
}

fn e2e_guard() -> std::sync::MutexGuard<'static, ()> {
    E2E_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn tester_command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-tester"));
    command
        .current_dir(root)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env_remove("GITHUB_ACTIONS");
    command
}

fn run_tester(root: &Path, args: &[&str]) -> Output {
    tester_command(root)
        .args(args)
        .output()
        .expect("cargo-tester should run")
}

fn cargo_command(root: &Path) -> Command {
    let mut command = env::var_os("CARGO").map_or_else(|| Command::new("cargo"), Command::new);
    command
        .current_dir(root)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", root.join("target"));
    command
}

fn compile_fixture(root: &Path) -> Output {
    cargo_command(root)
        .args(["test", "--no-run", "--quiet"])
        .output()
        .expect("fixture compilation should run")
}

fn measure_native(root: &Path) -> Duration {
    let started = Instant::now();
    let output = cargo_command(root)
        .args(["test", "--lib", "--quiet"])
        .output()
        .expect("native benchmark should run");
    let elapsed = started.elapsed();
    assert_success(&output, "native cargo test benchmark");
    elapsed
}

fn measure_isolated(root: &Path) -> Duration {
    let started = Instant::now();
    let output = run_tester(root, &["--ci", "--lib"]);
    let elapsed = started.elapsed();
    assert_success(&output, "cargo-tester benchmark");
    elapsed
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed:\n{}",
        output_text(output)
    );
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json_contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::Array(values) => values.iter().any(|value| json_contains_text(value, needle)),
        Value::Object(values) => values
            .values()
            .any(|value| json_contains_text(value, needle)),
        Value::String(value) => value.contains(needle),
        _ => false,
    }
}
