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
static NPM_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bnpm_[A-Za-z0-9]{20,}\b").expect("npm token regex must be valid")
});

pub(crate) fn sanitize_text(value: &str, redact: bool) -> String {
    let value = escape_controls(value, false);
    if !redact {
        return value;
    }

    let mut sanitized = value;
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
    sanitized = NPM_TOKEN
        .replace_all(&sanitized, "<redacted-token>")
        .into_owned();
    SENSITIVE_ASSIGNMENT
        .replace_all(&sanitized, "$1$2<redacted>")
        .into_owned()
}

pub(crate) fn sanitize_path(value: &str, redact: bool) -> String {
    let normalized = escape_controls(&value.replace('\\', "/"), true);
    if !redact {
        return normalized;
    }

    let path = Path::new(value);
    if !path.is_absolute() {
        return normalized;
    }

    if let Ok(workspace) = env::current_dir() {
        if let Some(relative) = strip_path(path, &workspace) {
            return escape_controls(&relative, true);
        }
    }

    path.file_name().map_or_else(
        || "<external>".to_owned(),
        |name| {
            format!(
                "<external>/{}",
                escape_controls(&name.to_string_lossy(), true)
            )
        },
    )
}

fn escape_controls(value: &str, single_line: bool) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if (character.is_control() && (single_line || !matches!(character, '\n' | '\t')))
            || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            result.extend(character.escape_default());
        } else {
            result.push(character);
        }
    }
    result
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

    #[test]
    fn neutralizes_terminal_controls_even_without_redaction() {
        let text = sanitize_text("\u{1b}]52;c;private\u{7}\rspoof", false);
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
        assert!(!text.contains('\r'));
        assert_eq!(sanitize_path("file\nspoof.rs", false), "file\\nspoof.rs");
    }
}
