use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum IssueHistoryReason {
    Recreated,
    Deleted,
    ManualArchive,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardProjectIssueHistory {
    pub id: Uuid,
    pub board_project_id: Uuid,
    pub issue_number: i32,
    pub issue_node_id: String,
    pub issue_url: String,
    pub reason: IssueHistoryReason,
    pub replaced_by_issue_node_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
