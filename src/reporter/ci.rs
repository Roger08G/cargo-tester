use crate::{
    privacy::{sanitize_path, sanitize_text},
    runner::TestResult,
};

pub fn render_github_annotations(results: &[TestResult]) -> Option<String> {
    let annotations = results
        .iter()
        .filter(|result| result.status.is_failure())
        .map(|result| {
            let panic = result
                .failure
                .as_ref()
                .and_then(|failure| failure.panic.as_ref());
            let file = panic.map_or(result.file.as_str(), |panic| panic.file.as_str());
            let file = sanitize_path(file, true);
            let line = panic.map(|panic| panic.line).or(result.line);
            // Panic text and assertion values are arbitrary user data; pattern
            // redaction cannot guarantee they contain no secrets.
            let message = "Rust test failed";
            let mut properties = format!(
                "file={},title={}",
                escape_property(&file),
                escape_property(&format!(
                    "Test failed: {}",
                    sanitize_text(&result.full_name, true)
                ))
            );
            if let Some(line) = line {
                properties.push_str(&format!(",line={line}"));
            }
            format!("::error {properties}::{}", escape_data(message))
        })
        .collect::<Vec<_>>();

    (!annotations.is_empty()).then(|| annotations.join("\n"))
}

fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}
