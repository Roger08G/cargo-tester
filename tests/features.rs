use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cargo_tester::{
    cli::Cli,
    config::TesterConfig,
    history::History,
    reporter::{
        ReportFilter, render_github_annotations, render_history, render_report_with_filter,
        write_report,
    },
    runner::{FailureOutput, PanicDetails, TestResult, TestStatus},
    state::{load_failed_tests, load_last_run, write_last_run, write_last_run_with_options},
};

mod cli_features {
    use super::*;

    #[test]
    fn separates_tester_cargo_and_harness_arguments() {
        let cli = Cli::parse_from([
            "tester",
            "--ci",
            "--last-failed",
            "--group=database",
            "--package",
            "api",
            "--features=metrics,telemetry",
            "auth::tests",
            "--",
            "--nocapture",
            "--test-threads=1",
        ])
        .expect("arguments should parse");

        assert!(cli.ci);
        assert!(cli.last_failed);
        assert_eq!(cli.group.as_deref(), Some("database"));
        assert_eq!(cli.filter.as_deref(), Some("auth::tests"));
        assert_eq!(
            cli.cargo_args,
            ["--package", "api", "--features=metrics,telemetry"]
        );
        assert_eq!(cli.harness_args, ["--nocapture", "--test-threads=1"]);
    }

    #[test]
    fn rejects_conflicting_history_and_reserved_harness_options() {
        let history = Cli::parse_from(["--history", "--failed"])
            .expect_err("history should be a standalone operation");
        let harness = Cli::parse_from(["--", "--format=json"])
            .expect_err("managed harness options should be rejected");

        assert!(history.to_string().contains("cannot be combined"));
        assert!(harness.to_string().contains("managed by cargo-tester"));
    }

    #[test]
    fn rejects_duplicate_groups_and_missing_cargo_values() {
        let duplicate = Cli::parse_from(["--group", "unit", "--group=integration"])
            .expect_err("duplicate groups should fail");
        let missing = Cli::parse_from(["--package", "--failed"])
            .expect_err("missing Cargo values should fail");

        assert!(duplicate.to_string().contains("only be used once"));
        assert!(missing.to_string().contains("requires a value"));
    }
}

mod config_features {
    use super::*;

    #[test]
    fn parses_groups_execution_output_and_history_settings() {
        let config = TesterConfig::parse(
            r#"
            [output]
            max-width = 100
            show-passed = false
            show-failed = true
            show-ignored = false

            [execution]
            jobs = 3

            [history]
            enabled = true
            max-runs = 20
            show-runs-history = 5
            regression-threshold-percent = 25
            minimum-test-duration = "10ms"

            [groups.database]
            patterns = ["database::*", "integration::storage::*"]
            sequential = true
            "#,
        )
        .expect("extended config should parse");

        assert_eq!(config.output.max_width, Some(100));
        assert!(!config.output.show_passed);
        assert!(!config.output.show_ignored);
        assert_eq!(config.execution.resolved_jobs(), 3);
        assert_eq!(config.history.max_runs, 20);
        assert_eq!(
            config.history.minimum_test_duration(),
            Duration::from_millis(10)
        );
        assert!(
            config
                .group("database")
                .unwrap()
                .matches("database::connects")
                .unwrap()
        );
        assert!(config.is_sequential_test("database::connects").unwrap());
        assert!(!config.is_sequential_test("parser::works").unwrap());
    }

    #[test]
    fn validates_group_patterns_and_table_width() {
        let group = TesterConfig::parse(
            r#"
            [groups.invalid]
            patterns = ["["]
            "#,
        )
        .expect_err("invalid globs should fail");
        let width = TesterConfig::parse(
            r#"
            [output]
            max-width = 40
            "#,
        )
        .expect_err("unusable widths should fail");

        assert!(group.to_string().contains("group `invalid`"));
        assert!(width.to_string().contains("max-width"));
    }

    #[test]
    fn ci_defaults_disable_terminal_only_styling() {
        let mut config = TesterConfig::parse(
            r#"
            [output]
            color = true
            unicode = true
            emoji = true
            solution = true
            "#,
        )
        .unwrap();

        config.apply_ci_defaults();

        assert!(!config.output.color);
        assert!(!config.output.unicode);
        assert!(!config.output.emoji);
        assert!(config.output.solution);
    }
}

mod persistence_features {
    use super::*;

    #[test]
    fn persists_only_last_failed_test_names() {
        let root = temp_dir("last_failed");
        let results = vec![
            result(1, TestStatus::Pass, 10),
            result(2, TestStatus::Fail, 20),
        ];

        write_last_run(&results, &root).expect("last run should be written");
        let failed = load_failed_tests(&root)
            .expect("last run should load")
            .expect("state should exist");

        assert_eq!(failed, HashSet::from(["suite::case_2".to_owned()]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persists_options_needed_to_reproduce_the_previous_run() {
        let root = temp_dir("last_failed_options");
        let cargo_args = vec!["--features".to_owned(), "demo-failures".to_owned()];
        let harness_args = vec!["--include-ignored".to_owned()];

        write_last_run_with_options(
            &[result(1, TestStatus::Fail, 10)],
            &root,
            &cargo_args,
            &harness_args,
        )
        .unwrap();
        let previous = load_last_run(&root).unwrap().unwrap();

        assert_eq!(previous.cargo_args, cargo_args);
        assert_eq!(previous.harness_args, harness_args);
        assert_eq!(
            previous.failed_tests,
            HashSet::from(["suite::case_1".to_owned()])
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stores_bounded_history_and_detects_real_regressions() {
        let root = temp_dir("history");
        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            unicode = false

            [history]
            max-runs = 2
            show-runs-history = 2
            regression-threshold-percent = 25
            minimum-test-duration = "10ms"
            "#,
        )
        .unwrap();
        let mut history = History::default();
        history
            .record(
                &[result(1, TestStatus::Pass, 100)],
                Duration::from_millis(100),
                &config.history,
                &root,
            )
            .unwrap();

        let regressions = history.regressions(&[result(1, TestStatus::Pass, 150)], &config.history);
        assert_eq!(regressions.len(), 1);
        assert!((regressions[0].increase_percent - 50.0).abs() < 1e-9);

        history
            .record(
                &[result(1, TestStatus::Pass, 150)],
                Duration::from_millis(150),
                &config.history,
                &root,
            )
            .unwrap();
        history
            .record(
                &[result(1, TestStatus::Pass, 160)],
                Duration::from_millis(160),
                &config.history,
                &root,
            )
            .unwrap();

        let loaded = History::load(&root).unwrap();
        assert_eq!(loaded.runs().len(), 2);
        assert!(render_history(&loaded, &config).contains("Date (UTC)"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_canonical_summary_and_removes_legacy_file() {
        let root = temp_dir("summary_migration");
        fs::write(root.join("sumary.txt"), "legacy").unwrap();

        write_report("current", &root).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("summary.txt")).unwrap(),
            "current\n"
        );
        assert!(!root.join("sumary.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

mod reporter_features {
    use super::*;

    #[test]
    fn status_visibility_does_not_change_summary_totals() {
        let config = TesterConfig::parse(
            r#"
            [output]
            color = false
            unicode = false
            show-passed = false
            show-ignored = false
            "#,
        )
        .unwrap();
        let results = vec![
            result(1, TestStatus::Pass, 10),
            result(2, TestStatus::Fail, 20),
            result(3, TestStatus::Ignored, 5),
        ];

        let report = render_report_with_filter(
            &results,
            Duration::from_millis(35),
            &config,
            ReportFilter::All,
        );

        assert!(!report.contains("case_1"));
        assert!(report.contains("case_2"));
        assert!(!report.contains("case_3"));
        assert!(report.contains("1 passed | 1 failed | 1 ignored"));
    }

    #[test]
    fn github_annotations_include_exact_failure_location() {
        let mut failed = result(1, TestStatus::Fail, 20);
        failed.file = "tests\\example.rs".to_owned();
        failed.failure = Some(FailureOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(101),
            panic: Some(PanicDetails {
                file: "tests\\example.rs".to_owned(),
                line: 14,
                column: Some(9),
                message: Some("expected 1, got 2".to_owned()),
            }),
        });

        let output = render_github_annotations(&[failed]).unwrap();

        assert!(output.starts_with("::error file=tests/example.rs"));
        assert!(output.contains("line=14"));
        assert!(output.ends_with("::expected 1, got 2"));
    }
}

fn result(id: usize, status: TestStatus, duration_ms: u64) -> TestResult {
    TestResult {
        id,
        full_name: format!("suite::case_{id}"),
        file: "tests/features.rs".to_owned(),
        family: "suite".to_owned(),
        test: format!("case_{id}"),
        line: Some(id),
        source_line: Some(format!("fn case_{id}() {{")),
        status,
        duration: Duration::from_millis(duration_ms),
        executable: PathBuf::from("target/debug/deps/features"),
        target_source: PathBuf::from("tests/features.rs"),
        failure: None,
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "cargo_tester_features_{name}_{}_{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
