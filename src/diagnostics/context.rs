use std::fs;

use crate::{
    formatting::source_path,
    runner::{FailureOutput, TestResult},
};

use super::{
    AssertionDetails, AssertionKind, Confidence, FailureCategory, FailureDiagnosis,
    SourceContextLine,
};

const SOURCE_CONTEXT_RADIUS: usize = 4;

pub(super) struct DiagnosticContext<'a> {
    pub result: &'a TestResult,
    pub target: String,
    pub message: String,
    pub output_lower: String,
    pub source: String,
    pub panic_expression: String,
    pub assertion: Option<AssertionDetails>,
}

impl<'a> DiagnosticContext<'a> {
    pub fn new(result: &'a TestResult, failure: &FailureOutput) -> Self {
        let output = combined_output(failure);
        let source_lines = panic_source_lines(failure);
        Self {
            result,
            target: failure_target(result, failure),
            message: failure_message(failure, &output),
            output_lower: output.to_ascii_lowercase(),
            source: source_lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            panic_expression: panic_expression_text(&source_lines),
            assertion: parse_assertion_details(failure),
        }
    }

    pub fn diagnosis(
        &self,
        category: FailureCategory,
        confidence: Confidence,
        summary: String,
        action_items: Vec<String>,
    ) -> FailureDiagnosis {
        FailureDiagnosis {
            category,
            confidence,
            summary,
            evidence: evidence_items(&self.target, &self.message, &self.source),
            action_items,
        }
    }

    pub fn output_contains_any(&self, needles: &[&str]) -> bool {
        contains_any(&self.output_lower, needles)
    }

    pub fn message_contains_any(&self, needles: &[&str]) -> bool {
        contains_any(&self.message, needles)
    }

    pub fn rerun_action(&self) -> String {
        format!(
            "Reejecuta `cargo test {} -- --exact`.",
            self.result.full_name
        )
    }

    pub fn assertion_values(&self, assertion: &AssertionDetails) -> String {
        match (&assertion.left, &assertion.right) {
            (Some(left), Some(right)) => {
                format!(" Valor real: `{left}`; esperado: `{right}`.")
            }
            _ => String::new(),
        }
    }

    pub fn contains_check(&self) -> Option<ContainsCheck> {
        extract_contains_check(&self.panic_expression)
    }
}

pub fn parse_assertion_details(failure: &FailureOutput) -> Option<AssertionDetails> {
    let output = combined_output(failure);
    let kind = if output.contains("assertion `left == right` failed") {
        AssertionKind::AssertEq
    } else if output.contains("assertion `left != right` failed") {
        AssertionKind::AssertNe
    } else if output.contains("assertion failed") {
        AssertionKind::Assert
    } else {
        return None;
    };

    Some(AssertionDetails {
        kind,
        left: prefixed_output_value(&output, "left:"),
        right: prefixed_output_value(&output, "right:"),
    })
}

pub(crate) fn source_line_at(file: &str, line: usize) -> Option<String> {
    fs::read_to_string(file).ok().and_then(|contents| {
        contents
            .lines()
            .nth(line.saturating_sub(1))
            .map(str::trim)
            .map(str::to_owned)
    })
}

pub(crate) fn source_context_at(file: &str, line: usize, radius: usize) -> Vec<SourceContextLine> {
    let Ok(contents) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let start = line.saturating_sub(radius).max(1);
    let end = line.saturating_add(radius);

    contents
        .lines()
        .enumerate()
        .filter_map(|(index, text)| {
            let current_line = index + 1;
            (start..=end)
                .contains(&current_line)
                .then(|| SourceContextLine {
                    line: current_line,
                    text: text.trim().to_owned(),
                    is_panic_line: current_line == line,
                })
        })
        .collect()
}

pub(super) struct ContainsCheck {
    pub collection: Option<String>,
    pub expected: String,
}

fn failure_target(result: &TestResult, failure: &FailureOutput) -> String {
    failure
        .panic
        .as_ref()
        .map(|panic| source_path(&panic.file, Some(panic.line)))
        .unwrap_or_else(|| source_path(&result.file, result.line))
}

fn failure_message(failure: &FailureOutput, output: &str) -> String {
    if let Some(message) = failure
        .panic
        .as_ref()
        .and_then(|panic| panic.message.as_deref())
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return compact_text(message, 220);
    }

    if output.contains("test did not panic as expected") {
        return "test did not panic as expected".to_owned();
    }

    output
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && (line.contains("panicked at")
                    || line.contains("FAILED")
                    || line.contains("assertion"))
        })
        .map(|line| compact_text(line, 220))
        .unwrap_or_else(|| "test failed without a captured panic message".to_owned())
}

fn panic_source_lines(failure: &FailureOutput) -> Vec<SourceContextLine> {
    failure
        .panic
        .as_ref()
        .map(|panic| source_context_at(&panic.file, panic.line, SOURCE_CONTEXT_RADIUS))
        .unwrap_or_default()
}

fn panic_expression_text(lines: &[SourceContextLine]) -> String {
    lines
        .iter()
        .skip_while(|line| !line.is_panic_line)
        .take(4)
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn combined_output(failure: &FailureOutput) -> String {
    match (failure.stdout.is_empty(), failure.stderr.is_empty()) {
        (false, false) => format!("{}\n{}", failure.stdout, failure.stderr),
        (false, true) => failure.stdout.clone(),
        (true, false) => failure.stderr.clone(),
        (true, true) => String::new(),
    }
}

fn prefixed_output_value(output: &str, prefix: &str) -> Option<String> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix(prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn extract_contains_check(context: &str) -> Option<ContainsCheck> {
    context.lines().find_map(|line| {
        let contains_index = line.find(".contains(")?;
        let expected = first_string_literal(&line[contains_index..])?;
        let collection = line[..contains_index]
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .rfind(|part| !part.is_empty())
            .map(str::to_owned);

        Some(ContainsCheck {
            collection,
            expected,
        })
    })
}

fn first_string_literal(text: &str) -> Option<String> {
    if let Some(raw_start) = text.find('r') {
        let raw = &text[raw_start + 1..];
        let hashes = raw
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if raw.as_bytes().get(hashes) == Some(&b'"') {
            let content_start = raw_start + 1 + hashes + 1;
            let terminator = format!("\"{}", "#".repeat(hashes));
            let end = text[content_start..].find(&terminator)?;
            return Some(text[content_start..content_start + end].to_owned());
        }
    }

    let start = text.find('"')? + 1;
    let mut escaped = false;
    for (offset, character) in text[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' => escaped = true,
            '"' => return Some(text[start..start + offset].to_owned()),
            _ => {}
        }
    }

    None
}

fn evidence_items(target: &str, message: &str, context: &str) -> Vec<String> {
    let mut evidence = vec![format!("panic target: {target}")];
    if !message.trim().is_empty() {
        evidence.push(format!("panic message: {message}"));
    }
    if !context.trim().is_empty() {
        evidence.push(format!(
            "source context: {}",
            compact_text(&context.replace(['\r', '\n'], " | "), 600)
        ));
    }
    evidence
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let mut compact = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    compact.push_str("...");
    compact
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}
