use std::time::Duration;

use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ContentArrangement, Table, presets::ASCII_FULL,
    presets::UTF8_FULL,
};

use crate::{
    config::TesterConfig,
    formatting,
    privacy::sanitize_path,
    runner::{TestResult, TestStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFilter {
    All,
    FailedOnly,
}

pub fn render_report(results: &[TestResult], total: Duration, config: &TesterConfig) -> String {
    render_report_with_filter(results, total, config, ReportFilter::All)
}

pub fn render_report_with_filter(
    results: &[TestResult],
    total: Duration,
    config: &TesterConfig,
    filter: ReportFilter,
) -> String {
    let mut table = Table::new();
    table.load_preset(if config.output.unicode {
        UTF8_FULL
    } else {
        ASCII_FULL
    });
    table.set_content_arrangement(ContentArrangement::Dynamic);
    if let Some(width) = config.output.max_width {
        table.set_width(width);
    }
    table.set_header(
        ["ID", "Path", "Family", "Test", "Status", "Time"]
            .map(|label| header_cell(label, config.output.color)),
    );

    for result in results
        .iter()
        .filter(|result| filter.includes(result, config))
    {
        table.add_row(vec![
            Cell::new(format!("#{}", result.id)).set_alignment(CellAlignment::Right),
            muted_cell(
                &sanitize_path(&result.file, config.privacy.redact),
                config.output.color,
            ),
            muted_cell(&result.family, config.output.color),
            Cell::new(&result.test),
            status_cell(result.status, config.output.color),
            duration_cell(result.duration, config),
        ]);
    }

    format!(
        "{table}\n\n{}",
        summary_line(results, total, config.output.unicode, config.output.color)
    )
}

impl ReportFilter {
    fn includes(self, result: &TestResult, config: &TesterConfig) -> bool {
        match self {
            Self::All => match result.status {
                TestStatus::Pass => config.output.show_passed,
                TestStatus::Fail => config.output.show_failed,
                TestStatus::Ignored => config.output.show_ignored,
                TestStatus::Timeout => config.output.show_failed,
            },
            Self::FailedOnly => result.status.is_failure(),
        }
    }
}

pub(super) fn status_counts(results: &[TestResult]) -> (usize, usize, usize, usize) {
    results.iter().fold(
        (0, 0, 0, 0),
        |(passed, failed, ignored, timed_out), result| match result.status {
            TestStatus::Pass => (passed + 1, failed, ignored, timed_out),
            TestStatus::Fail => (passed, failed + 1, ignored, timed_out),
            TestStatus::Ignored => (passed, failed, ignored + 1, timed_out),
            TestStatus::Timeout => (passed, failed, ignored, timed_out + 1),
        },
    )
}

fn header_cell(label: &str, color: bool) -> Cell {
    let cell = Cell::new(label);
    if color {
        cell.add_attribute(Attribute::Bold).fg(Color::Magenta)
    } else {
        cell
    }
}

fn muted_cell(value: &str, color: bool) -> Cell {
    let cell = Cell::new(value);
    if color {
        cell.fg(Color::DarkGrey)
    } else {
        cell
    }
}

fn status_cell(status: TestStatus, color: bool) -> Cell {
    let cell = Cell::new(status.label());
    if !color {
        return cell;
    }

    let cell = cell.add_attribute(Attribute::Bold);
    match status {
        TestStatus::Pass => cell.fg(Color::Green),
        TestStatus::Fail => cell.fg(Color::Red),
        TestStatus::Ignored => cell.fg(Color::Yellow),
        TestStatus::Timeout => cell.fg(Color::Red).add_attribute(Attribute::Bold),
    }
}

fn duration_cell(duration: Duration, config: &TesterConfig) -> Cell {
    let cell = Cell::new(formatting::duration(duration)).set_alignment(CellAlignment::Right);
    if !config.output.color {
        return cell;
    }

    if duration >= config.timing.very_slow_threshold() {
        cell.fg(Color::Red).add_attribute(Attribute::Bold)
    } else if duration >= config.timing.slow_threshold() {
        cell.fg(Color::Yellow)
    } else {
        cell.fg(Color::DarkGrey)
    }
}

fn summary_line(results: &[TestResult], total: Duration, unicode: bool, color: bool) -> String {
    let (passed, failed, ignored, timed_out) = status_counts(results);
    let separator = if unicode { " \u{00B7} " } else { " | " };

    format!(
        "{}{separator}{}{separator}{}{separator}{}{separator}Total: {}",
        summary_part(passed, "passed", "32", color),
        summary_part(failed, "failed", "31", color),
        summary_part(timed_out, "timed out", "31", color),
        summary_part(ignored, "ignored", "33", color),
        formatting::duration(total)
    )
}

fn summary_part(count: usize, label: &str, color_code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{color_code}m{count}\x1b[0m {label}")
    } else {
        format!("{count} {label}")
    }
}
