mod ci;
mod details;
mod history;
mod json;
mod table;

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub use ci::render_github_annotations;
pub use details::render_failure_details;
pub use history::{render_history, render_regressions};
pub use json::write_details_json;
pub use table::{ReportFilter, render_report, render_report_with_filter};

const SUMMARY_FILE_NAME: &str = "summary.txt";
const LEGACY_SUMMARY_FILE_NAME: &str = "sumary.txt";
const DETAILS_FILE_NAME: &str = "details.json";
const DETAILS_RECOMMENDATION: &str =
    "Recomendado usar --details para ver los errores de los tests.";

pub fn render_details_recommendation(config: &crate::config::TesterConfig) -> String {
    let emoji = if config.output.emoji {
        "\u{2139}\u{FE0F} "
    } else {
        ""
    };
    let message = format!("{emoji}{DETAILS_RECOMMENDATION}");

    if config.output.color {
        format!("\x1b[34m{message}\x1b[0m")
    } else {
        message
    }
}

pub fn write_report(report: &str, output_path: &Path) -> Result<()> {
    write_output_file(output_path, SUMMARY_FILE_NAME, report)?;
    remove_output_file(output_path, LEGACY_SUMMARY_FILE_NAME)
}

pub fn remove_details_json(output_path: &Path) -> Result<()> {
    remove_output_file(output_path, DETAILS_FILE_NAME)
}

fn remove_output_file(output_path: &Path, file_name: &str) -> Result<()> {
    let path = output_path.join(file_name);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn write_output_file(output_path: &Path, file_name: &str, contents: &str) -> Result<()> {
    fs::create_dir_all(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;

    let path = output_path.join(file_name);
    let contents = with_trailing_newline(contents);
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn with_trailing_newline(contents: &str) -> String {
    let mut normalized = contents.trim_end_matches(['\r', '\n']).to_owned();
    normalized.push('\n');
    normalized
}

fn details_path(output_path: &Path) -> PathBuf {
    output_path.join(DETAILS_FILE_NAME)
}
