use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cargo_tester::{
    cli::Cli,
    config::TesterConfig,
    reporter::{
        ReportFilter, remove_details_json, render_details_recommendation, render_failure_details,
        render_report, render_report_with_filter, write_details_json, write_report,
    },
    runner::{FailureOutput, PanicDetails, TestResult, TestStatus, has_failures},
    source::SourceIndex,
};

mod cli_tests {
    use super::*;

    #[test]
    fn accepts_cargo_subcommand_arguments() {
        let cli = Cli::parse_from([
            "tester",
            "sandbox::tests",
            "--details",
            "--failed",
            "--no-color",
        ])
        .expect("CLI arguments should parse");

        assert_eq!(cli.filter.as_deref(), Some("sandbox::tests"));
        assert!(cli.details);
        assert!(cli.failed);
        assert!(cli.no_color);
        assert!(!cli.help);
        assert!(!cli.version);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = Cli::parse_from(["--unknown"]).expect_err("unknown options should fail");

        assert!(error.to_string().contains("unknown option"));
    }

    #[test]
    fn rejects_more_than_one_filter() {
        let error = Cli::parse_from(["first", "second"])
            .expect_err("multiple positional filters should fail");

        assert!(error.to_string().contains("unexpected extra argument"));
    }

    #[test]
    fn parses_help_and_version_flags() {
        let help = Cli::parse_from(["--help"]).expect("help should parse");
        let version = Cli::parse_from(["-V"]).expect("version should parse");

        assert!(help.help);
        assert!(version.version);
    }
}

mod config_tests {
    use super::*;

    #[test]
    fn uses_defaults_for_an_empty_config() {
        let config = TesterConfig::parse("").expect("empty config should use defaults");

        assert!(config.output.color);
        assert!(!config.output.emoji);
        assert!(config.output.unicode);
        assert_eq!(
            config.output.output_path,
            PathBuf::from(".cargo/tester-output/")
        );
        assert_eq!(config.timing.slow_threshold(), Duration::from_secs(1));
        assert_eq!(config.timing.very_slow_threshold(), Duration::from_secs(5));
        assert_eq!(config.execution.test_timeout(), Duration::from_secs(300));
        assert_eq!(
            config.execution.discovery_timeout(),
            Duration::from_secs(900)
        );
        assert_eq!(config.execution.max_output_bytes, 256 * 1024);
        assert_eq!(
            config.execution.max_discovery_output_bytes,
            16 * 1024 * 1024
        );
        assert!(!config.privacy.redact);
    }

    #[test]
    fn reads_output_and_timing_values() {
        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            emoji = true
            unicode = false
            output-path = "target/custom-tester-output"

            [timing]
            slow-threshold = "250ms"
            very-slow-threshold = "2.5s"
            "#,
        )
        .expect("config should parse");

        assert!(!config.output.color);
        assert!(config.output.emoji);
        assert!(!config.output.unicode);
        assert_eq!(
            config.output.output_path,
            PathBuf::from("target/custom-tester-output")
        );
        assert_eq!(config.timing.slow_threshold(), Duration::from_millis(250));
        assert_eq!(
            config.timing.very_slow_threshold(),
            Duration::from_millis(2500)
        );
    }

    #[test]
    fn rejects_invalid_timing_values() {
        let invalid_duration = TesterConfig::parse(
            r#"
            [timing]
            slow-threshold = "fast"
            "#,
        )
        .expect_err("invalid durations should fail");
        assert!(invalid_duration.to_string().contains("invalid duration"));

        let invalid_order = TesterConfig::parse(
            r#"
            [timing]
            slow-threshold = "5s"
            very-slow-threshold = "1s"
            "#,
        )
        .expect_err("invalid threshold order should fail");
        assert!(invalid_order.to_string().contains("slow-threshold"));
    }

    #[test]
    fn validates_execution_limits() {
        let config = TesterConfig::parse(
            r#"
            [execution]
            jobs = 2
            test-timeout = "750ms"
            discovery-timeout = "30s"
            max-discovery-output-bytes = 1048576
            max-output-bytes = 4096

            [privacy]
            include-captured-output = false
            include-source-context = false
            redact = true
            "#,
        )
        .expect("execution limits should parse");

        assert_eq!(config.execution.resolved_jobs(), 2);
        assert_eq!(config.execution.test_timeout(), Duration::from_millis(750));
        assert_eq!(
            config.execution.discovery_timeout(),
            Duration::from_secs(30)
        );
        assert_eq!(config.execution.max_output_bytes, 4096);
        assert_eq!(config.execution.max_discovery_output_bytes, 1_048_576);
        assert!(!config.privacy.include_captured_output);
        assert!(!config.privacy.include_source_context);
        assert!(config.privacy.redact);

        for contents in [
            "[execution]\ntest-timeout = \"0s\"",
            "[execution]\ndiscovery-timeout = \"0s\"",
            "[execution]\nmax-output-bytes = 0",
            "[execution]\nmax-discovery-output-bytes = 0",
        ] {
            assert!(TesterConfig::parse(contents).is_err());
        }
    }

    #[test]
    fn rejects_unknown_fields_and_empty_output_paths() {
        let unknown = TesterConfig::parse(
            r#"
            [output]
            colors = true
            "#,
        )
        .expect_err("unknown fields should fail");
        let empty_path = TesterConfig::parse(
            r#"
            [output]
            output-path = ""
            "#,
        )
        .expect_err("empty output paths should fail");
        let removed_solution = TesterConfig::parse(
            r#"
            [output]
            solution = true
            "#,
        )
        .expect_err("removed solution setting should fail");

        assert!(unknown.to_string().contains("unknown field"));
        assert!(empty_path.to_string().contains("output-path"));
        assert!(removed_solution.to_string().contains("unknown field"));
    }

    #[test]
    fn creates_a_plain_file_configuration_without_changing_other_options() {
        let config = TesterConfig::parse(
            r#"
            [output]
            color = true
            emoji = true
            unicode = true
            "#,
        )
        .expect("config should parse");

        let plain = config.for_plain_file();

        assert!(!plain.output.color);
        assert!(!plain.output.emoji);
        assert!(!plain.output.unicode);
        assert!(config.output.color, "original config must remain unchanged");
    }

    #[test]
    fn loads_config_from_project_root_first() {
        let root = create_temp_project("root_config");
        write_config(
            &root.join("tester.toml"),
            r#"
            [output]
            color = false
            unicode = false
            "#,
        );
        write_config(
            &root.join(".cargo").join("tester.toml"),
            r#"
            [output]
            color = true
            unicode = true
            "#,
        );

        let config = TesterConfig::load_from_root(&root).expect("config should load");

        assert!(!config.output.color);
        assert!(!config.output.unicode);

        fs::remove_dir_all(root).expect("temp project should be removed");
    }

    #[test]
    fn falls_back_to_cargo_config_directory() {
        let root = create_temp_project("cargo_config");
        write_config(
            &root.join(".cargo").join("tester.toml"),
            r#"
            [output]
            color = false
            unicode = false

            [timing]
            slow-threshold = "500ms"
            very-slow-threshold = "2s"
            "#,
        );

        let config = TesterConfig::load_from_root(&root).expect("config should load");

        assert!(!config.output.color);
        assert!(!config.output.unicode);
        assert_eq!(config.timing.slow_threshold(), Duration::from_millis(500));
        assert_eq!(config.timing.very_slow_threshold(), Duration::from_secs(2));

        fs::remove_dir_all(root).expect("temp project should be removed");
    }
}

mod runner_tests {
    use super::*;

    #[test]
    fn exposes_clear_status_labels() {
        assert_eq!(TestStatus::Pass.label(), "PASS");
        assert_eq!(TestStatus::Fail.label(), "FAIL");
        assert_eq!(TestStatus::Ignored.label(), "IGNORED");
        assert_eq!(TestStatus::Timeout.label(), "TIMEOUT");
    }

    #[test]
    fn detects_failed_results() {
        let results = vec![
            fake_result(1, TestStatus::Pass),
            fake_result(2, TestStatus::Fail),
        ];

        assert!(has_failures(&results));
        assert!(!has_failures(&[fake_result(1, TestStatus::Pass)]));
    }
}

mod reporter_tests {
    use super::*;

    #[test]
    fn report_without_color_contains_no_ansi_sequences() {
        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            unicode = false
            "#,
        )
        .expect("config should parse");
        let results = vec![
            fake_result(1, TestStatus::Pass),
            fake_result(2, TestStatus::Fail),
        ];

        let report = render_report(&results, Duration::from_millis(20), &config);

        assert!(!report.contains('\x1b'));
        assert!(report.contains("| ID "));
        assert!(report.contains("| PASS "));
        assert!(report.contains("| FAIL "));
        assert!(report.contains("\n\n1 passed | 1 failed | 0 timed out | 0 ignored"));
    }

    #[test]
    fn failed_report_shows_only_failed_rows_and_keeps_total_summary() {
        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            unicode = false
            "#,
        )
        .expect("config should parse");
        let results = vec![
            fake_result(1, TestStatus::Pass),
            failed_result(2),
            fake_result(3, TestStatus::Ignored),
        ];

        let report = render_report_with_filter(
            &results,
            Duration::from_millis(45),
            &config,
            ReportFilter::FailedOnly,
        );

        assert!(!report.contains("case_1"));
        assert!(!report.contains("case_3"));
        assert!(report.contains("fails_when_expected_patch_is_missing"));
        assert!(report.contains("\n\n1 passed | 1 failed | 0 timed out | 1 ignored"));
    }

    #[test]
    fn details_recommendation_respects_color_and_emoji_settings() {
        let plain = TesterConfig::parse(
            r#"
            [output]
            color = false
            emoji = false
            "#,
        )
        .expect("plain config should parse");
        let styled = TesterConfig::parse(
            r#"
            [output]
            color = true
            emoji = true
            "#,
        )
        .expect("styled config should parse");

        assert_eq!(
            render_details_recommendation(&plain),
            "Recomendado usar --details para ver los errores de los tests."
        );
        assert_eq!(
            render_details_recommendation(&styled),
            "\x1b[34m\u{2139}\u{FE0F} Recomendado usar --details para ver los errores de los tests.\x1b[0m"
        );
    }

    #[test]
    fn details_include_failed_tests_only() {
        let root = create_temp_project("details");
        let failed = failed_result(2);
        let results = vec![fake_result(1, TestStatus::Pass), failed];

        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            unicode = false
            "#,
        )
        .expect("config should parse");
        let details =
            render_failure_details(&results, &config).expect("failed tests should render details");

        assert!(details.contains("[#2] fails_when_expected_patch_is_missing"));
        assert!(details.contains("sandbox::tests"));
        assert!(details.contains("Path: tests/example.rs:L42"));
        assert!(details.contains("Duration: 15 ms"));
        assert!(details.contains("Function: fn fails_when_expected_patch_is_missing()"));
        assert!(details.contains("Panic"));
        assert!(details.contains("| - Captured stdout\n|\n"));
        assert!(!details.contains("#1 PASS"));

        let mut color_config = config.clone();
        color_config.output.color = true;
        let colored_details = render_failure_details(&results, &color_config)
            .expect("colored failed tests should render details");

        assert!(colored_details.contains("\x1b[34mfails_when_expected_patch_is_missing\x1b[0m"));
        assert!(colored_details.contains("\x1b[1;35mFamily:\x1b[0m sandbox::tests"));
        assert!(colored_details.contains("\x1b[1;35mMessage:\x1b[0m demo failure"));
        assert!(!colored_details.contains("\x1b[90msandbox::tests\x1b[0m"));
        assert!(colored_details.contains("\x1b[90mthread panicked\x1b[0m"));
        assert!(!colored_details.contains("\x1b[1;31mFAIL\x1b[0m"));

        let mut emoji_config = config.clone();
        emoji_config.output.emoji = true;
        let emoji_details = render_failure_details(&results, &emoji_config)
            .expect("emoji failed tests should render details");

        assert!(emoji_details.contains("[#2] \u{1F9EA} fails_when_expected_patch_is_missing"));
        assert!(!details.contains("Posible solucion:"));

        write_details_json(&results, Duration::from_millis(25), &root, &config.privacy)
            .expect("details json should be written");
        let json =
            fs::read_to_string(root.join("details.json")).expect("details json should exist");
        let json: serde_json::Value =
            serde_json::from_str(&json).expect("details json should be valid");

        assert_eq!(
            json["failed"]
                .as_array()
                .expect("failed should be an array")
                .len(),
            1
        );
        assert_eq!(json["failed"][0]["id"], 2);
        assert_eq!(json["failed"][0]["location"]["line"], 42);
        assert_eq!(
            json["failed"][0]["location"]["path"],
            "tests/example.rs:L42"
        );
        assert_eq!(json["failed"][0]["panic"]["line"], 45);
        assert_eq!(json["schema_version"], 4);
        assert_eq!(json["summary"]["total"], 2);
        assert_eq!(json["summary"]["failed"], 1);
        assert_eq!(json["summary"]["successful"], false);
        assert_eq!(json["summary"]["total_duration_ms"], 25);
        assert_eq!(json["summary"]["cumulative_test_duration_ms"], 25);
        assert_eq!(json["failed"][0]["exit_code"], 101);
        assert_eq!(
            json["failed"][0]["target"]["source_path"],
            "tests/example.rs"
        );
        assert_eq!(
            json["failed"][0]["rerun"]["cargo_tester"],
            "cargo tester sandbox::tests::fails_when_expected_patch_is_missing"
        );
        assert!(json["failed"][0].get("solution").is_none());
        assert!(json["failed"][0].get("solution_details").is_none());
        assert!(json["failed"][0]["panic"]["source_context"].is_array());
        assert!(
            json["tests"]
                .as_array()
                .expect("tests should be an array")
                .len()
                == 2
        );

        fs::remove_dir_all(root).expect("temp project should be removed");
    }

    #[test]
    fn details_json_includes_objective_assertion_details() {
        let root = create_temp_project("assertion_details");
        let results = vec![assert_eq_failed_result(1)];

        write_details_json(
            &results,
            Duration::from_millis(8),
            &root,
            &TesterConfig::default().privacy,
        )
        .expect("details json should be written");

        let json =
            fs::read_to_string(root.join("details.json")).expect("details json should exist");
        let json: serde_json::Value =
            serde_json::from_str(&json).expect("details json should be valid");
        let failed = &json["failed"][0];

        assert_eq!(failed["assertion"]["kind"], "assert_eq");
        assert_eq!(failed["assertion"]["left"], "2");
        assert_eq!(failed["assertion"]["right"], "3");
        assert!(failed.get("solution").is_none());
        assert!(failed.get("solution_details").is_none());

        fs::remove_dir_all(root).expect("temp project should be removed");
    }

    #[test]
    fn report_files_have_one_final_newline_and_stale_details_are_removed() {
        let root = create_temp_project("report_files");

        write_report("report\n\n", &root).expect("summary should be written");
        fs::write(root.join("details.json"), "stale").expect("stale details should be created");
        remove_details_json(&root).expect("stale details should be removed");

        let summary =
            fs::read_to_string(root.join("summary.txt")).expect("summary should be readable");
        assert_eq!(summary, "report\n");
        assert!(!root.join("details.json").exists());
        remove_details_json(&root).expect("removing a missing file should be harmless");

        fs::remove_dir_all(root).expect("temp project should be removed");
    }
}

mod source_tests {
    use super::*;

    #[test]
    fn finds_the_file_for_an_integration_test() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let sources = SourceIndex::build(root.clone()).expect("source index should build");
        let target_source = root.join("tests").join("core.rs");

        let file = sources
            .location_for("finds_the_file_for_an_integration_test", &target_source)
            .file;

        assert!(file.ends_with("tests\\core.rs") || file.ends_with("tests/core.rs"));
    }

    #[test]
    fn indexes_single_line_and_qualified_test_attributes() {
        let root = create_temp_project("source_attributes");
        let test_file = root.join("tests").join("attributes.rs");
        write_config(
            &test_file,
            "#[test] fn same_line() {}\n#[tokio::test]\nasync fn qualified() {}\n",
        );

        let sources = SourceIndex::build(root.clone()).expect("source index should build");
        let same_line = sources.location_for("same_line", &test_file);
        let qualified = sources.location_for("qualified", &test_file);

        assert_eq!(same_line.line, Some(1));
        assert_eq!(qualified.line, Some(3));
        assert_eq!(
            qualified.source_line.as_deref(),
            Some("async fn qualified() {}")
        );

        fs::remove_dir_all(root).expect("temp project should be removed");
    }

    #[test]
    fn resolves_duplicate_function_names_using_the_target_source() {
        let root = create_temp_project("duplicate_sources");
        let first = root.join("tests").join("first.rs");
        let second = root.join("tests").join("second.rs");
        write_config(&first, "#[test]\nfn duplicate_name() {}\n");
        write_config(&second, "\n\n#[test]\nfn duplicate_name() {}\n");

        let sources = SourceIndex::build(root.clone()).expect("source index should build");
        let location = sources.location_for("duplicate_name", &second);

        assert!(
            location.file.ends_with("tests\\second.rs")
                || location.file.ends_with("tests/second.rs")
        );
        assert_eq!(location.line, Some(4));

        fs::remove_dir_all(root).expect("temp project should be removed");
    }
}

fn fake_result(id: usize, status: TestStatus) -> TestResult {
    TestResult {
        id,
        full_name: format!("runner_tests::case_{id}"),
        file: "tests/core.rs".to_owned(),
        family: "runner_tests".to_owned(),
        test: format!("case_{id}"),
        line: Some(1),
        source_line: Some(format!("fn case_{id}()")),
        status,
        duration: Duration::from_millis(10),
        executable: PathBuf::from("target/debug/deps/core-test"),
        target_source: PathBuf::from("tests/core.rs"),
        failure: None,
    }
}

fn failed_result(id: usize) -> TestResult {
    TestResult {
        id,
        full_name: "sandbox::tests::fails_when_expected_patch_is_missing".to_owned(),
        file: "tests/example.rs".to_owned(),
        family: "sandbox::tests".to_owned(),
        test: "fails_when_expected_patch_is_missing".to_owned(),
        line: Some(42),
        source_line: Some("fn fails_when_expected_patch_is_missing() {".to_owned()),
        status: TestStatus::Fail,
        duration: Duration::from_millis(15),
        executable: PathBuf::from("target/debug/deps/example-test"),
        target_source: PathBuf::from("tests/example.rs"),
        failure: Some(FailureOutput {
            stdout: "thread panicked".to_owned(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            exit_code: Some(101),
            panic: Some(PanicDetails {
                file: "tests/example.rs".to_owned(),
                line: 45,
                column: Some(13),
                message: Some("demo failure".to_owned()),
            }),
        }),
    }
}

fn assert_eq_failed_result(id: usize) -> TestResult {
    TestResult {
        id,
        full_name: "sandbox::tests::fails_when_discovered_test_count_is_wrong".to_owned(),
        file: "tests/example.rs".to_owned(),
        family: "sandbox::tests".to_owned(),
        test: "fails_when_discovered_test_count_is_wrong".to_owned(),
        line: Some(21),
        source_line: Some("fn fails_when_discovered_test_count_is_wrong() {".to_owned()),
        status: TestStatus::Fail,
        duration: Duration::from_millis(8),
        executable: PathBuf::from("target/debug/deps/example-test"),
        target_source: PathBuf::from("tests/example.rs"),
        failure: Some(FailureOutput {
            stdout: "\
thread 'sandbox::tests::fails_when_discovered_test_count_is_wrong' panicked at tests\\example.rs:25:13:
assertion `left == right` failed: demo failure: expected one more discovered test
  left: 2
 right: 3
"
            .to_owned(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            exit_code: Some(101),
            panic: Some(PanicDetails {
                file: "tests/example.rs".to_owned(),
                line: 25,
                column: Some(13),
                message: Some(
                    "assertion `left == right` failed: demo failure: expected one more discovered test"
                        .to_owned(),
                ),
            }),
        }),
    }
}

fn create_temp_project(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "cargo_tester_{name}_{}_{}",
        std::process::id(),
        unique
    ));

    fs::create_dir_all(root.join(".cargo")).expect("temp project should be created");
    root
}

fn write_config(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("config directory should be created");
    }

    fs::write(path, contents).expect("config should be written");
}
