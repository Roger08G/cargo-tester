use crate::{
    config::TesterConfig,
    formatting,
    privacy::{sanitize_path, sanitize_text},
    runner::{FailureOutput, TestResult},
};

pub fn render_failure_details(results: &[TestResult], config: &TesterConfig) -> Option<String> {
    let failures = results
        .iter()
        .filter(|result| result.status.is_failure())
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
        &formatting::source_path(&sanitize_path(&result.file, style.redact), result.line),
    );
    append_field(
        details,
        style,
        "Duration",
        &formatting::duration(result.duration),
    );

    if style.include_source_context {
        if let Some(source_line) = &result.source_line {
            append_field(
                details,
                style,
                "Function",
                &sanitize_text(&formatting::function_signature(source_line), style.redact),
            );
        }
    }

    if let Some(failure) = &result.failure {
        append_failure_output(details, failure, style);
    }

    details.push_str(&style.card_footer());
    details.push('\n');
}

fn append_failure_output(details: &mut String, failure: &FailureOutput, style: &DetailStyle) {
    if let Some(panic) = &failure.panic {
        append_section(details, style, "Panic");
        if let Some(message) = &panic.message {
            append_field(
                details,
                style,
                "Message",
                &sanitize_text(message, style.redact),
            );
        }
    }

    if style.include_captured_output {
        append_captured_output(
            details,
            style,
            "Captured stdout",
            &failure.stdout,
            failure.stdout_truncated,
        );
        append_captured_output(
            details,
            style,
            "Captured stderr",
            &failure.stderr,
            failure.stderr_truncated,
        );
    }
}

fn append_captured_output(
    details: &mut String,
    style: &DetailStyle,
    label: &str,
    output: &str,
    truncated: bool,
) {
    let output = output.trim();
    if output.is_empty() {
        return;
    }

    let label = if truncated {
        format!("{label} (truncated at configured limit)")
    } else {
        label.to_owned()
    };
    append_section(details, style, &label);
    append_blank_line(details, style);
    for line in output.lines() {
        details.push_str(style.vertical);
        details.push_str("   ");
        details.push_str(&style.muted(&sanitize_text(line, style.redact)));
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

struct DetailStyle {
    color: bool,
    emoji: bool,
    include_captured_output: bool,
    include_source_context: bool,
    redact: bool,
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
                include_captured_output: config.privacy.include_captured_output,
                include_source_context: config.privacy.include_source_context,
                redact: config.privacy.redact,
                top_left: "\u{256D}",
                bottom_left: "\u{2570}",
                horizontal: "\u{2500}",
                vertical: "\u{2502}",
            }
        } else {
            Self {
                color: config.output.color,
                emoji: config.output.emoji,
                include_captured_output: config.privacy.include_captured_output,
                include_source_context: config.privacy.include_source_context,
                redact: config.privacy.redact,
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
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}
