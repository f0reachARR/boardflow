use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ArtifactBundleStatus {
    Pending,
    Validating,
    Importing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct ArtifactBundle {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub intake_mode: String,
    pub staging_object_key: Option<String>,
    pub original_filename: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub status: ArtifactBundleStatus,
    pub error_message: Option<String>,
    pub received_at: DateTime<Utc>,
    pub validated_at: Option<DateTime<Utc>>,
    pub delete_after: Option<DateTime<Utc>>,
}
