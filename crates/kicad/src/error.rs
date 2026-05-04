use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KicadError {
    #[error("KiCad CLI command failed: {command} (exit code: {exit_code})")]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("KiCad CLI command timed out after {timeout_secs}s: {command}")]
    Timeout { command: String, timeout_secs: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("No .kicad_pro found in {0}")]
    NoKicadPro(PathBuf),

    #[error("Multiple .kicad_pro found in {0}")]
    MultipleKicadPro(PathBuf),

    #[error("No .kicad_pcb found for {stem} in {dir}")]
    NoKicadPcb { dir: PathBuf, stem: String },

    #[error("Multiple .kicad_pcb found in {dir} (no unique match for stem {stem})")]
    MultipleKicadPcb { dir: PathBuf, stem: String },

    #[error("No .kicad_sch found for {stem} in {dir}")]
    NoKicadSch { dir: PathBuf, stem: String },

    #[error("Multiple .kicad_sch found in {dir} (no unique match for stem {stem})")]
    MultipleKicadSch { dir: PathBuf, stem: String },

    #[error("Config validation error: {0}")]
    ConfigValidation(String),

    #[error("No .boardflow.yml found")]
    NoBoardflowYml,
}

pub type Result<T> = std::result::Result<T, KicadError>;
