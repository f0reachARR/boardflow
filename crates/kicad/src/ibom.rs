use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::{KicadError, Result};

/// Run InteractiveHtmlBom generation via xvfb-run.
///
/// Returns the path to the generated HTML file on success.
pub async fn run_ibom(pcb_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    let pcb = pcb_path.to_str().unwrap_or_default();
    let out = output_dir.to_str().unwrap_or_default();

    let output = Command::new("xvfb-run")
        .args([
            "generate_interactive_bom",
            "--no-browser",
            "--dest-dir",
            out,
            pcb,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let exit_code = output.status.code().unwrap_or(-1);
        return Err(KicadError::CommandFailed {
            command: format!(
                "xvfb-run generate_interactive_bom --no-browser --dest-dir {out} {pcb}"
            ),
            exit_code,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Find the generated .html file in output_dir
    let entries = std::fs::read_dir(output_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "html" {
                    return Ok(path);
                }
            }
        }
    }

    Err(KicadError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "iBOM HTML file not found in output directory",
    )))
}
