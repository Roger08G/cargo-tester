use std::{env, path::Path, sync::LazyLock};

use regex::Regex;

static SENSITIVE_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b([a-z0-9_-]*(?:api[_-]?key|access[_-]?token|authorization|auth[_-]?token|bearer|client[_-]?secret|password|secret|token))\b(\s*[:=]\s*|\s+)(?:"[^"\r\n]*"|'[^'\r\n]*'|(?:bearer|basic)\s+[^\s,\r\n]+|[^\s,\r\n]+)"#,
    )
    .expect("sensitive assignment regex must be valid")
});
static GITHUB_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b")
        .expect("GitHub token regex must be valid")
});
static AWS_ACCESS_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("AWS access key regex must be valid")
});

pub(crate) fn sanitize_text(value: &str, redact: bool) -> String {
    if !redact {
        return value.to_owned();
    }

    let mut sanitized = value.to_owned();
    if let Ok(workspace) = env::current_dir() {
        sanitized = replace_path(&sanitized, &workspace, "<workspace>");
    }
    if let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
        sanitized = replace_path(&sanitized, Path::new(&home), "<home>");
    }
    sanitized = GITHUB_TOKEN
        .replace_all(&sanitized, "<redacted-token>")
        .into_owned();
    sanitized = AWS_ACCESS_KEY
        .replace_all(&sanitized, "<redacted-access-key>")
        .into_owned();
    SENSITIVE_ASSIGNMENT
        .replace_all(&sanitized, "$1$2<redacted>")
        .into_owned()
}

pub(crate) fn sanitize_path(value: &str, redact: bool) -> String {
    let normalized = value.replace('\\', "/");
    if !redact {
        return normalized;
    }

    let path = Path::new(value);
    if !path.is_absolute() {
        return normalized;
    }

    if let Ok(workspace) = env::current_dir() {
        if let Some(relative) = strip_path(path, &workspace) {
            return relative;
        }
    }

    path.file_name().map_or_else(
        || "<external>".to_owned(),
        |name| format!("<external>/{}", name.to_string_lossy()),
    )
}

fn strip_path(path: &Path, base: &Path) -> Option<String> {
    path.strip_prefix(base)
        .ok()
        .map(|relative| relative.display().to_string().replace('\\', "/"))
}

fn replace_path(value: &str, path: &Path, replacement: &str) -> String {
    let native = path.display().to_string();
    let slashed = native.replace('\\', "/");
    value
        .replace(&native, replacement)
        .replace(&slashed, replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secret_shapes() {
        let text = "DATABASE_PASSWORD=visible Authorization: Bearer private-value ghp_abcdefghijklmnopqrstuvwxyz123456 AKIA1234567890ABCDEF";
        let sanitized = sanitize_text(text, true);

        assert!(!sanitized.contains("visible"));
        assert!(!sanitized.contains("private-value"));
        assert!(!sanitized.contains("ghp_"));
        assert!(!sanitized.contains("AKIA"));
    }

    #[test]
    fn converts_workspace_paths_to_relative_paths() {
        let workspace = env::current_dir().unwrap();
        let path = workspace.join("src").join("lib.rs");

        assert_eq!(
            sanitize_path(&path.display().to_string(), true),
            "src/lib.rs"
        );
    }
}
