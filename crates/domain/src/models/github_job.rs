use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum GithubJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct GithubJob {
    pub id: Uuid,
    pub installation_id: i64,
    pub repository_id: Uuid,
    pub board_project_id: Option<Uuid>,
    pub board_run_id: Option<Uuid>,
    pub r#type: String,
    pub payload_json: serde_json::Value,
    pub status: GithubJobStatus,
    pub attempts: i32,
    pub run_after: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
