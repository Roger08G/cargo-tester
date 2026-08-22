use std::fs;

use serde::Serialize;

use crate::runner::FailureOutput;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct AssertionDetails {
    pub kind: AssertionKind,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AssertionKind {
    Assert,
    AssertEq,
    AssertNe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SourceContextLine {
    pub line: usize,
    pub text: String,
    pub is_panic_line: bool,
}

pub(super) fn parse_assertion_details(failure: &FailureOutput) -> Option<AssertionDetails> {
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

pub(super) fn source_line_at(file: &str, line: usize) -> Option<String> {
    fs::read_to_string(file).ok().and_then(|contents| {
        contents
            .lines()
            .nth(line.saturating_sub(1))
            .map(str::trim)
            .map(str::to_owned)
    })
}

pub(super) fn source_context_at(file: &str, line: usize, radius: usize) -> Vec<SourceContextLine> {
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
