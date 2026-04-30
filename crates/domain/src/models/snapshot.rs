use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardProjectSnapshot {
    pub id: Uuid,
    pub board_project_id: Uuid,
    pub board_run_id: Uuid,
    pub tree_hash: String,
    pub commit_sha: String,
    pub file_hashes_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardRunDiffMetadata {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub file_hashes_json: Option<serde_json::Value>,
    pub bom_summary_json: Option<serde_json::Value>,
    pub checks_summary_json: Option<serde_json::Value>,
    pub artifacts_summary_json: Option<serde_json::Value>,
    pub previews_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum BoardRunDiffStatus {
    Ready,
    NoBaseline,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardRunDiff {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub base_board_run_id: Option<Uuid>,
    pub status: BoardRunDiffStatus,
    pub summary_json: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}
