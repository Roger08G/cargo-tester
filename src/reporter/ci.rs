use crate::runner::{TestResult, TestStatus};

pub fn render_github_annotations(results: &[TestResult]) -> Option<String> {
    let annotations = results
        .iter()
        .filter(|result| result.status == TestStatus::Fail)
        .map(|result| {
            let panic = result
                .failure
                .as_ref()
                .and_then(|failure| failure.panic.as_ref());
            let file = panic.map_or(result.file.as_str(), |panic| panic.file.as_str());
            let file = file.replace('\\', "/");
            let line = panic.map(|panic| panic.line).or(result.line);
            let message = panic
                .and_then(|panic| panic.message.as_deref())
                .unwrap_or("Rust test failed");
            let mut properties = format!(
                "file={},title={}",
                escape_property(&file),
                escape_property(&format!("Test failed: {}", result.full_name))
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
