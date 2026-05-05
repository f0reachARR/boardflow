use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::Result;

/// Emit a GitHub Actions error annotation.
pub fn error(msg: &str) {
    eprintln!("::error::{msg}");
}

/// Emit a GitHub Actions warning annotation.
pub fn warning(msg: &str) {
    eprintln!("::warning::{msg}");
}

/// Append a key=value pair to the GITHUB_OUTPUT file.
pub fn set_output(key: &str, value: &str, path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{key}={value}")?;
    Ok(())
}

/// Write a Markdown job summary table.
pub fn write_job_summary(results: &[ProjectResult], summary_path: &Path) -> Result<()> {
    if summary_path.as_os_str().is_empty() {
        return Ok(());
    }

    let mut md = String::new();
    md.push_str("## BoardFlow Results\n\n");
    md.push_str("| Project | Status | Error |\n");
    md.push_str("|---------|--------|-------|\n");

    for r in results {
        let error_col = r.error.as_deref().unwrap_or("-");
        md.push_str(&format!("| {} | {} | {} |\n", r.path, r.status, error_col));
    }

    if let Some(parent) = summary_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)?;
    write!(file, "{md}")?;
    Ok(())
}

/// Write a summary for unsupported events (e.g. pull_request).
pub fn write_unsupported_event_summary(event: &str, summary_path: &Path) -> Result<()> {
    if summary_path.as_os_str().is_empty() {
        return Ok(());
    }

    let md = format!(
        "## BoardFlow\n\n> **Skipped**: Event `{event}` is not supported by BoardFlow Action.\n\nBoardFlow processes `push` and `workflow_dispatch` events only.\n"
    );

    if let Some(parent) = summary_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)?;
    write!(file, "{md}")?;
    Ok(())
}

pub struct ProjectResult {
    pub path: String,
    pub status: String,
    pub error: Option<String>,
}
