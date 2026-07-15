use std::env;

use crate::{
    config::TesterConfig,
    diagnostics::analyze_failure,
    formatting,
    runner::{FailureOutput, TestResult, TestStatus},
};

pub fn render_failure_details(results: &[TestResult], config: &TesterConfig) -> Option<String> {
    let failures = results
        .iter()
        .filter(|result| result.status == TestStatus::Fail)
        .collect::<Vec<_>>();

    if failures.is_empty() {
        return None;
    }

    let style = DetailStyle::from_config(config);
    let mut details = format!(
        "{}\n",
        style.heading(&format!("Failure details ({})", failures.len()))
    );

    for result in failures {
        append_failure_card(&mut details, result, &style);
    }

    Some(details)
}

fn append_failure_card(details: &mut String, result: &TestResult, style: &DetailStyle) {
    details.push('\n');
    details.push_str(&style.card_header(result.id, &result.test));
    details.push('\n');
    append_field(details, style, "Family", &result.family);
    append_field(details, style, "Test", &result.test);
    append_field(details, style, "Full name", &result.full_name);
    append_field(
        details,
        style,
        "Path",
        &formatting::source_path(&result.file, result.line),
    );
    append_field(
        details,
        style,
        "Duration",
        &formatting::duration(result.duration),
    );

    if let Some(source_line) = &result.source_line {
        append_field(
            details,
            style,
            "Function",
            &formatting::function_signature(source_line),
        );
    }

    if let Some(failure) = &result.failure {
        append_failure_output(details, result, failure, style);
    }

    details.push_str(&style.card_footer());
    details.push('\n');
}

fn append_failure_output(
    details: &mut String,
    result: &TestResult,
    failure: &FailureOutput,
    style: &DetailStyle,
) {
    if let Some(panic) = &failure.panic {
        append_section(details, style, "Panic");
        if let Some(message) = &panic.message {
            append_field(details, style, "Message", message);
        }
    }

    append_captured_output(details, style, "Captured stdout", &failure.stdout);
    append_captured_output(details, style, "Captured stderr", &failure.stderr);

    if style.solution {
        append_solution(details, style, result, failure);
    }
}

fn append_captured_output(details: &mut String, style: &DetailStyle, label: &str, output: &str) {
    let output = output.trim();
    if output.is_empty() {
        return;
    }

    append_section(details, style, label);
    append_blank_line(details, style);
    for line in output.lines() {
        details.push_str(style.vertical);
        details.push_str("   ");
        details.push_str(&style.muted(line));
        details.push('\n');
    }
    append_blank_line(details, style);
}

fn append_field(details: &mut String, style: &DetailStyle, label: &str, value: &str) {
    details.push_str(style.vertical);
    details.push(' ');
    details.push_str(&style.label(&format!("{label}:")));
    details.push(' ');
    details.push_str(value);
    details.push('\n');
}

fn append_section(details: &mut String, style: &DetailStyle, label: &str) {
    details.push_str(style.vertical);
    details.push(' ');
    details.push_str(&style.section_marker());
    details.push(' ');
    details.push_str(&style.section(label));
    details.push('\n');
}

fn append_blank_line(details: &mut String, style: &DetailStyle) {
    details.push_str(style.vertical);
    details.push('\n');
}

fn append_solution(
    details: &mut String,
    style: &DetailStyle,
    result: &TestResult,
    failure: &FailureOutput,
) {
    let diagnosis = analyze_failure(result, failure);
    let marker = if style.emoji { "\u{1F4A1} " } else { "" };
    let text = format!("{marker}Posible solucion: {}", diagnosis.summary);
    let continuation_indent = " ".repeat(formatting::display_width(marker));

    for line in wrap_text(&text, solution_width(), &continuation_indent) {
        details.push_str(style.vertical);
        details.push(' ');
        details.push_str(&style.solution_text(&line));
        details.push('\n');
    }
}

fn solution_width() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.parse::<usize>().ok())
        .map(|columns| columns.saturating_mul(80) / 100)
        .map(|columns| columns.min(96))
        .filter(|columns| *columns >= 60)
        .unwrap_or(80)
}

fn wrap_text(text: &str, max_width: usize, continuation_indent: &str) -> Vec<String> {
    let max_width = max_width.max(40);
    let indent_width = formatting::display_width(continuation_indent);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = formatting::display_width(word);
        let separator_width = usize::from(!current.is_empty());
        if !current.is_empty() && current_width + separator_width + word_width > max_width {
            lines.push(current);
            current = continuation_indent.to_owned();
            current.push_str(word);
            current_width = indent_width + word_width;
            continue;
        }

        if !current.is_empty() {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

struct DetailStyle {
    color: bool,
    emoji: bool,
    solution: bool,
    top_left: &'static str,
    bottom_left: &'static str,
    horizontal: &'static str,
    vertical: &'static str,
}

impl DetailStyle {
    fn from_config(config: &TesterConfig) -> Self {
        if config.output.unicode {
            Self {
                color: config.output.color,
                emoji: config.output.emoji,
                solution: config.output.solution,
                top_left: "\u{256D}",
                bottom_left: "\u{2570}",
                horizontal: "\u{2500}",
                vertical: "\u{2502}",
            }
        } else {
            Self {
                color: config.output.color,
                emoji: config.output.emoji,
                solution: config.output.solution,
                top_left: "+",
                bottom_left: "+",
                horizontal: "-",
                vertical: "|",
            }
        }
    }

    fn heading(&self, text: &str) -> String {
        paint(text, "1;33", self.color)
    }

    fn card_header(&self, id: usize, test: &str) -> String {
        let emoji = if self.emoji { "\u{1F9EA} " } else { "" };
        format!(
            "{}{} {} {emoji}{}",
            paint(self.top_left, "90", self.color),
            paint(&self.horizontal.repeat(2), "90", self.color),
            self.id(id),
            paint(test, "34", self.color)
        )
    }

    fn card_footer(&self) -> String {
        format!(
            "{}{}",
            paint(self.bottom_left, "90", self.color),
            paint(&self.horizontal.repeat(72), "90", self.color)
        )
    }

    fn label(&self, text: &str) -> String {
        paint(text, "1;35", self.color)
    }

    fn id(&self, id: usize) -> String {
        if self.color {
            format!(
                "{}{}{}",
                paint("[", "90", true),
                paint(&format!("#{id}"), "37", true),
                paint("]", "90", true)
            )
        } else {
            format!("[#{id}]")
        }
    }

    fn muted(&self, text: &str) -> String {
        paint(text, "90", self.color)
    }

    fn section(&self, text: &str) -> String {
        paint(text, "1;33", self.color)
    }

    fn section_marker(&self) -> String {
        paint(self.horizontal, "90", self.color)
    }

    fn solution_text(&self, text: &str) -> String {
        paint(text, "3;32", self.color)
    }
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}
