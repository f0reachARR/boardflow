use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

use crate::{KicadError, Result};

pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct KicadCli {
    pub bin_path: PathBuf,
    pub timeout: Duration,
}

impl KicadCli {
    pub fn new() -> Self {
        Self {
            bin_path: PathBuf::from("kicad-cli"),
            timeout: Duration::from_secs(300),
        }
    }

    pub fn with_bin_path(bin_path: impl Into<PathBuf>) -> Self {
        Self {
            bin_path: bin_path.into(),
            timeout: Duration::from_secs(300),
        }
    }

    pub async fn run_erc(&self, sch_file: &Path, output_json: &Path) -> Result<CommandOutput> {
        let sch = sch_file.to_str().unwrap_or_default();
        let out = output_json.to_str().unwrap_or_default();
        self.exec_erc_drc(&["sch", "erc", "--format", "json", "--output", out, sch])
            .await
    }

    pub async fn run_drc(&self, pcb_file: &Path, output_json: &Path) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_json.to_str().unwrap_or_default();
        self.exec_erc_drc(&["pcb", "drc", "--format", "json", "--output", out, pcb])
            .await
    }

    pub async fn export_pcb_pdf(&self, pcb_file: &Path, output_pdf: &Path) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_pdf.to_str().unwrap_or_default();
        self.exec(&["pcb", "export", "pdf", "--output", out, pcb])
            .await
    }

    pub async fn export_sch_pdf(&self, sch_file: &Path, output_pdf: &Path) -> Result<CommandOutput> {
        let sch = sch_file.to_str().unwrap_or_default();
        let out = output_pdf.to_str().unwrap_or_default();
        self.exec(&["sch", "export", "pdf", "--output", out, sch])
            .await
    }

    pub async fn export_pcb_svg(
        &self,
        pcb_file: &Path,
        output_svg: &Path,
        layers: &str,
    ) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_svg.to_str().unwrap_or_default();
        self.exec(&["pcb", "export", "svg", "--layers", layers, "--output", out, pcb])
            .await
    }

    pub async fn export_gerbers(
        &self,
        pcb_file: &Path,
        output_dir: &Path,
    ) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_dir.to_str().unwrap_or_default();
        self.exec(&["pcb", "export", "gerbers", "--output", out, pcb])
            .await
    }

    pub async fn export_drill(
        &self,
        pcb_file: &Path,
        output_dir: &Path,
    ) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_dir.to_str().unwrap_or_default();
        self.exec(&["pcb", "export", "drill", "--output", out, pcb])
            .await
    }

    pub async fn export_bom(&self, sch_file: &Path, output_csv: &Path) -> Result<CommandOutput> {
        let sch = sch_file.to_str().unwrap_or_default();
        let out = output_csv.to_str().unwrap_or_default();
        self.exec(&["sch", "export", "bom", "--output", out, sch])
            .await
    }

    pub async fn export_position(
        &self,
        pcb_file: &Path,
        output_csv: &Path,
    ) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_csv.to_str().unwrap_or_default();
        self.exec(&["pcb", "export", "pos", "--output", out, pcb])
            .await
    }

    pub async fn render_3d(
        &self,
        pcb_file: &Path,
        output_png: &Path,
        side: &str,
    ) -> Result<CommandOutput> {
        let pcb = pcb_file.to_str().unwrap_or_default();
        let out = output_png.to_str().unwrap_or_default();
        self.exec(&["pcb", "render", "--side", side, "--output", out, pcb])
            .await
    }

    async fn exec(&self, args: &[&str]) -> Result<CommandOutput> {
        let child = Command::new(&self.bin_path)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let command_str = format!("{} {}", self.bin_path.display(), args.join(" "));

        match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                if exit_code != 0 {
                    return Err(KicadError::CommandFailed {
                        command: command_str,
                        exit_code,
                        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    });
                }
                Ok(CommandOutput {
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }
            Ok(Err(e)) => Err(KicadError::Io(e)),
            Err(_) => {
                // On timeout, the child future is dropped which kills the process
                Err(KicadError::Timeout {
                    command: command_str,
                    timeout_secs: self.timeout.as_secs(),
                })
            }
        }
    }

    async fn exec_erc_drc(&self, args: &[&str]) -> Result<CommandOutput> {
        let child = Command::new(&self.bin_path)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let command_str = format!("{} {}", self.bin_path.display(), args.join(" "));

        match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                // ERC/DRC: exit code 5 means violations found, still success
                if exit_code != 0 && exit_code != 5 {
                    return Err(KicadError::CommandFailed {
                        command: command_str,
                        exit_code,
                        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    });
                }
                Ok(CommandOutput {
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }
            Ok(Err(e)) => Err(KicadError::Io(e)),
            Err(_) => {
                // On timeout, the child future is dropped which kills the process
                Err(KicadError::Timeout {
                    command: command_str,
                    timeout_secs: self.timeout.as_secs(),
                })
            }
        }
    }
}

impl Default for KicadCli {
    fn default() -> Self {
        Self::new()
    }
}
