use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ArtifactStatus {
    Available,
    Missing,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub r#type: String,
    pub status: ArtifactStatus,
    pub filename: Option<String>,
    pub source_path: Option<String>,
    pub logical_name: Option<String>,
    pub content_type: Option<String>,
    pub storage_key: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub status_reason: Option<String>,
    pub error_message: Option<String>,
    pub source_bundle_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
