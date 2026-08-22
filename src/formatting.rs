use std::time::Duration;

pub(crate) fn duration(value: Duration) -> String {
    if value < Duration::from_millis(1) {
        "<1 ms".to_owned()
    } else if value < Duration::from_secs(1) {
        format!("{} ms", value.as_millis())
    } else {
        format!("{:.2} s", value.as_secs_f64())
    }
}

pub(crate) fn source_path(file: &str, line: Option<usize>) -> String {
    line.map_or_else(|| file.to_owned(), |line| format!("{file}:L{line}"))
}

pub(crate) fn function_signature(source_line: &str) -> String {
    source_line.trim().trim_end_matches('{').trim().to_owned()
}
