use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

use crate::{KicadError, Result};

/// PCB side for SVG export and 3D render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcbSide {
    Top,
    Bottom,
}

impl PcbSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PcbSide::Top => "top",
            PcbSide::Bottom => "bottom",
        }
    }

    fn svg_layers(&self) -> &'static str {
        match self {
            PcbSide::Top => "F.Cu,F.Silkscreen,F.Mask,Edge.Cuts",
            PcbSide::Bottom => "B.Cu,B.Silkscreen,B.Mask,Edge.Cuts",
        }
    }
}

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
        let args = Self::build_erc_args(sch_file, output_json);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_erc_drc(&args_ref).await
    }

    pub async fn run_drc(&self, pcb_file: &Path, output_json: &Path) -> Result<CommandOutput> {
        let args = Self::build_drc_args(pcb_file, output_json);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_erc_drc(&args_ref).await
    }

    pub async fn export_pcb_pdf(&self, pcb_file: &Path, output_pdf: &Path) -> Result<CommandOutput> {
        let args = Self::build_pcb_pdf_args(pcb_file, output_pdf);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_sch_pdf(&self, sch_file: &Path, output_pdf: &Path) -> Result<CommandOutput> {
        let args = Self::build_sch_pdf_args(sch_file, output_pdf);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_pcb_svg(
        &self,
        pcb_file: &Path,
        output_svg: &Path,
        side: PcbSide,
    ) -> Result<CommandOutput> {
        let args = Self::build_pcb_svg_args(pcb_file, output_svg, side);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_gerbers(
        &self,
        pcb_file: &Path,
        output_dir: &Path,
    ) -> Result<CommandOutput> {
        let args = Self::build_gerbers_args(pcb_file, output_dir);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_drill(
        &self,
        pcb_file: &Path,
        output_dir: &Path,
    ) -> Result<CommandOutput> {
        let args = Self::build_drill_args(pcb_file, output_dir);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_bom(&self, sch_file: &Path, output_csv: &Path) -> Result<CommandOutput> {
        let args = Self::build_bom_args(sch_file, output_csv);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn export_position(
        &self,
        pcb_file: &Path,
        output_csv: &Path,
    ) -> Result<CommandOutput> {
        let args = Self::build_position_args(pcb_file, output_csv);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    pub async fn render_3d(
        &self,
        pcb_file: &Path,
        output_png: &Path,
        side: PcbSide,
    ) -> Result<CommandOutput> {
        let args = Self::build_render_3d_args(pcb_file, output_png, side);
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec(&args_ref).await
    }

    // --- Argument builders (pub for testing) ---

    pub fn build_erc_args(sch_file: &Path, output_json: &Path) -> Vec<String> {
        vec![
            "sch".into(),
            "erc".into(),
            "--format".into(),
            "json".into(),
            "--severity-all".into(),
            "--exit-code-violations".into(),
            "--output".into(),
            output_json.to_str().unwrap_or_default().into(),
            sch_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_drc_args(pcb_file: &Path, output_json: &Path) -> Vec<String> {
        vec![
            "pcb".into(),
            "drc".into(),
            "--format".into(),
            "json".into(),
            "--severity-all".into(),
            "--exit-code-violations".into(),
            "--output".into(),
            output_json.to_str().unwrap_or_default().into(),
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_pcb_pdf_args(pcb_file: &Path, output_pdf: &Path) -> Vec<String> {
        vec![
            "pcb".into(),
            "export".into(),
            "pdf".into(),
            "--layers".into(),
            "F.Cu,B.Cu,F.Silkscreen,B.Silkscreen,Edge.Cuts".into(),
            "--output".into(),
            output_pdf.to_str().unwrap_or_default().into(),
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_sch_pdf_args(sch_file: &Path, output_pdf: &Path) -> Vec<String> {
        vec![
            "sch".into(),
            "export".into(),
            "pdf".into(),
            "--output".into(),
            output_pdf.to_str().unwrap_or_default().into(),
            sch_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_pcb_svg_args(pcb_file: &Path, output_svg: &Path, side: PcbSide) -> Vec<String> {
        vec![
            "pcb".into(),
            "export".into(),
            "svg".into(),
            "--mode-multi".into(),
            "--layers".into(),
            side.svg_layers().into(),
            "--output".into(),
            output_svg.to_str().unwrap_or_default().into(),
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_gerbers_args(pcb_file: &Path, output_dir: &Path) -> Vec<String> {
        let mut out = output_dir.to_str().unwrap_or_default().to_string();
        if !out.ends_with('/') {
            out.push('/');
        }
        vec![
            "pcb".into(),
            "export".into(),
            "gerbers".into(),
            "--output".into(),
            out,
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_drill_args(pcb_file: &Path, output_dir: &Path) -> Vec<String> {
        let mut out = output_dir.to_str().unwrap_or_default().to_string();
        if !out.ends_with('/') {
            out.push('/');
        }
        vec![
            "pcb".into(),
            "export".into(),
            "drill".into(),
            "--format".into(),
            "excellon".into(),
            "--excellon-separate-th".into(),
            "--output".into(),
            out,
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_bom_args(sch_file: &Path, output_csv: &Path) -> Vec<String> {
        vec![
            "sch".into(),
            "export".into(),
            "bom".into(),
            "--output".into(),
            output_csv.to_str().unwrap_or_default().into(),
            sch_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_position_args(pcb_file: &Path, output_csv: &Path) -> Vec<String> {
        vec![
            "pcb".into(),
            "export".into(),
            "pos".into(),
            "--format".into(),
            "csv".into(),
            "--output".into(),
            output_csv.to_str().unwrap_or_default().into(),
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    pub fn build_render_3d_args(pcb_file: &Path, output_png: &Path, side: PcbSide) -> Vec<String> {
        vec![
            "pcb".into(),
            "render".into(),
            "--side".into(),
            side.as_str().into(),
            "--quality".into(),
            "basic".into(),
            "--output".into(),
            output_png.to_str().unwrap_or_default().into(),
            pcb_file.to_str().unwrap_or_default().into(),
        ]
    }

    async fn exec(&self, args: &[&str]) -> Result<CommandOutput> {
        let child = Command::new(&self.bin_path)
            .args(args)
            .kill_on_drop(true)
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
            Err(_) => Err(KicadError::Timeout {
                command: command_str,
                timeout_secs: self.timeout.as_secs(),
            }),
        }
    }

    async fn exec_erc_drc(&self, args: &[&str]) -> Result<CommandOutput> {
        let child = Command::new(&self.bin_path)
            .args(args)
            .kill_on_drop(true)
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
            Err(_) => Err(KicadError::Timeout {
                command: command_str,
                timeout_secs: self.timeout.as_secs(),
            }),
        }
    }
}

impl Default for KicadCli {
    fn default() -> Self {
        Self::new()
    }
}
