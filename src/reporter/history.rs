use comfy_table::{
    Attribute, Cell, Color, ContentArrangement, Table, presets::ASCII_FULL, presets::UTF8_FULL,
};
use std::time::{Duration, UNIX_EPOCH};

use crate::{
    config::TesterConfig,
    formatting,
    history::{History, PerformanceRegression},
};

pub fn render_history(history: &History, config: &TesterConfig) -> String {
    if history.runs().is_empty() {
        return "No test history available.".to_owned();
    }

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
        [
            "Run",
            "Date (UTC)",
            "Tests",
            "Passed",
            "Failed",
            "Timeouts",
            "Ignored",
            "Time",
        ]
        .map(|label| heading(label, config.output.color)),
    );

    for (index, run) in history
        .runs()
        .iter()
        .rev()
        .take(config.history.show_runs_history)
        .enumerate()
    {
        table.add_row([
            Cell::new(format!("#{}", index + 1)),
            Cell::new(format_timestamp(run.generated_at_unix_ms)),
            Cell::new(run.total),
            Cell::new(run.passed),
            Cell::new(run.failed),
            Cell::new(run.timed_out),
            Cell::new(run.ignored),
            Cell::new(formatting::duration(Duration::from_millis(
                run.total_duration_ms,
            ))),
        ]);
    }

    table.to_string()
}

fn format_timestamp(unix_ms: u128) -> String {
    let milliseconds = unix_ms.min(u128::from(u64::MAX)) as u64;
    humantime::format_rfc3339_millis(UNIX_EPOCH + Duration::from_millis(milliseconds)).to_string()
}

pub fn render_regressions(
    regressions: &[PerformanceRegression],
    config: &TesterConfig,
) -> Option<String> {
    if regressions.is_empty() {
        return None;
    }

    let heading = paint(
        &format!("Performance regressions ({})", regressions.len()),
        "1;33",
        config.output.color,
    );
    let marker = if config.output.unicode {
        "\u{2022}"
    } else {
        "-"
    };
    let lines = regressions
        .iter()
        .map(|regression| {
            format!(
                "{marker} {}: {} -> {} (+{:.1}%)",
                regression.full_name,
                formatting::duration(regression.previous_duration),
                formatting::duration(regression.current_duration),
                regression.increase_percent
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!("{heading}\n{lines}"))
}

fn heading(label: &str, color: bool) -> Cell {
    let cell = Cell::new(label);
    if color {
        cell.add_attribute(Attribute::Bold).fg(Color::Magenta)
    } else {
        cell
    }
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}
