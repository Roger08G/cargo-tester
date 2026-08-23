use std::{
    env,
    io::{self, IsTerminal},
    process::ExitCode,
    time::{Duration, Instant},
};

use anyhow::Result;

use crate::{
    cli::{self, Cli},
    config::TesterConfig,
    history::History,
    persistence::OutputLock,
    process::Cancellation,
    reporter::{
        ReportFilter, remove_details_json, render_details_recommendation, render_failure_details,
        render_github_annotations, render_history, render_regressions, render_report_with_filter,
        write_details_json, write_report,
    },
    runner::{DiscoveryOptions, ExecutionOptions, discover_tests, has_failures, run_tests},
    source::SourceIndex,
    state::{load_last_run, write_last_run_with_privacy},
};

pub fn run() -> Result<ExitCode> {
    run_with_cli(Cli::parse()?)
}

fn run_with_cli(cli: Cli) -> Result<ExitCode> {
    if cli.help {
        cli::print_help();
        return Ok(ExitCode::SUCCESS);
    }

    if cli.version {
        println!("cargo-tester {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    let mut config = TesterConfig::load()?;
    if cli.ci {
        config.apply_ci_defaults();
    }
    config.output.color &= terminal_supports_color(&cli);

    if cli.history {
        let _output_lock = OutputLock::acquire(&config.output.output_path)?;
        println!(
            "{}",
            render_history(&History::load(&config.output.output_path)?, &config)
        );
        return Ok(ExitCode::SUCCESS);
    }

    let _cancellation = Cancellation::install()?;

    let mut cargo_args = cli.cargo_args.clone();
    let mut harness_args = cli.harness_args.clone();
    let previous_failures = if cli.last_failed {
        match load_last_run(&config.output.output_path)? {
            None => {
                return finish_empty_run(
                    &config,
                    cli.details || cli.ci,
                    "No previous test execution was found.",
                );
            }
            Some(previous) if !previous.has_failures() => {
                return finish_empty_run(
                    &config,
                    cli.details || cli.ci,
                    "No tests failed in the previous execution.",
                );
            }
            Some(previous) => {
                if cargo_args.is_empty() {
                    cargo_args = previous.cargo_args.clone();
                }
                if harness_args.is_empty() {
                    harness_args = previous.harness_args.clone();
                }
                Some(previous)
            }
        }
    } else {
        None
    };

    let mut tests = discover_tests(DiscoveryOptions {
        filter: cli.filter.as_deref(),
        cargo_args: &cargo_args,
        harness_args: &harness_args,
        timeout: config.execution.discovery_timeout(),
        max_output_bytes: config.execution.max_discovery_output_bytes,
        redact_output: config.privacy.redact,
    })?;

    if let Some(group_name) = cli.group.as_deref() {
        let matcher = config.group(group_name)?.matcher()?;
        tests.retain(|test| matcher.is_match(&test.full_name));
    }
    if let Some(previous_failures) = &previous_failures {
        tests.retain(|test| previous_failures.matches(test));
    }

    if tests.is_empty() {
        let message = if cli.last_failed {
            "Previous failed tests do not match the current Cargo selection."
        } else {
            "No Rust tests matched."
        };
        return finish_empty_run(&config, cli.details || cli.ci, message);
    }

    let sequential_tests =
        config.sequential_test_names(tests.iter().map(|test| test.full_name.as_str()))?;
    let sources = SourceIndex::build(env::current_dir()?)?;
    let started = Instant::now();
    let results = run_tests(
        &tests,
        &sources,
        ExecutionOptions {
            harness_args: &harness_args,
            jobs: config.execution.resolved_jobs(),
            sequential_tests: &sequential_tests,
            timeout: config.execution.test_timeout(),
            max_output_bytes: config.execution.max_output_bytes,
        },
    )?;
    let total = started.elapsed();
    let _output_lock = OutputLock::acquire(&config.output.output_path)?;

    let mut history = if config.history.enabled {
        match History::load(&config.output.output_path) {
            Ok(history) => Some(history),
            Err(error) => {
                eprintln!("Warning: test history could not be loaded: {error:#}");
                Some(History::default())
            }
        }
    } else {
        None
    };
    let regressions = history
        .as_ref()
        .map(|history| history.regressions(&results, &config.history))
        .unwrap_or_default();
    let report_filter = report_filter(&cli);

    let mut terminal_report = render_report_with_filter(&results, total, &config, report_filter);
    append_regressions(&mut terminal_report, &regressions, &config);
    println!("{terminal_report}");

    let failed = has_failures(&results);
    if cli.ci && env::var_os("GITHUB_ACTIONS").is_some() {
        if let Some(annotations) = render_github_annotations(&results) {
            println!("\n{annotations}");
        }
    }
    if failed && !cli.details && !cli.ci {
        println!("\n{}", render_details_recommendation(&config));
    }

    let file_config = config.for_plain_file();
    let mut file_report = render_report_with_filter(&results, total, &file_config, report_filter);
    append_regressions(&mut file_report, &regressions, &file_config);
    if cli.details {
        append_failure_details(&mut file_report, &results, &file_config);
    }
    write_report(&file_report, &config.output.output_path)?;
    write_last_run_with_privacy(
        &results,
        &config.output.output_path,
        &cargo_args,
        &harness_args,
        &config.privacy,
    )?;

    if cli.details {
        if let Some(details) = render_failure_details(&results, &config) {
            println!("\n{details}");
        }
    }
    if cli.details || cli.ci {
        write_details_json(&results, total, &config.output.output_path, &config.privacy)?;
    } else {
        remove_details_json(&config.output.output_path)?;
    }

    if let Some(history) = &mut history {
        history.record(
            &results,
            total,
            &config.history,
            &config.privacy,
            &config.output.output_path,
        )?;
    }

    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn terminal_supports_color(cli: &Cli) -> bool {
    !cli.no_color && env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal()
}

fn report_filter(cli: &Cli) -> ReportFilter {
    if cli.failed {
        ReportFilter::FailedOnly
    } else {
        ReportFilter::All
    }
}

fn finish_empty_run(config: &TesterConfig, write_details: bool, message: &str) -> Result<ExitCode> {
    println!("{message}");
    let _output_lock = OutputLock::acquire(&config.output.output_path)?;
    write_report(message, &config.output.output_path)?;

    if write_details {
        write_details_json(
            &[],
            Duration::ZERO,
            &config.output.output_path,
            &config.privacy,
        )?;
    } else {
        remove_details_json(&config.output.output_path)?;
    }

    Ok(ExitCode::SUCCESS)
}

fn append_failure_details(
    report: &mut String,
    results: &[crate::runner::TestResult],
    config: &TesterConfig,
) {
    if let Some(details) = render_failure_details(results, config) {
        report.push_str("\n\n");
        report.push_str(&details);
    }
}

fn append_regressions(
    report: &mut String,
    regressions: &[crate::history::PerformanceRegression],
    config: &TesterConfig,
) {
    if let Some(regressions) = render_regressions(regressions, config) {
        report.push_str("\n\n");
        report.push_str(&regressions);
    }
}
