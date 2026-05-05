use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("Input error: {0}")]
    Input(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("KiCad error: {0}")]
    Kicad(#[from] boardflow_kicad::KicadError),

    #[error("Bundle error: {0}")]
    Bundle(String),

    #[error("Upload error: {0}")]
    Upload(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ActionError>;
