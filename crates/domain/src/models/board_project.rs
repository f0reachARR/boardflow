use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum IssueSyncStatus {
    Pending,
    Syncing,
    Synced,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardProject {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub issue_number: Option<i32>,
    pub issue_node_id: Option<String>,
    pub issue_url: Option<String>,
    pub issue_sync_status: IssueSyncStatus,
    pub dashboard_comment_id: Option<i64>,
    pub recreate_issue_on_update: bool,
    pub latest_tree_hash: Option<String>,
    pub latest_completed_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
