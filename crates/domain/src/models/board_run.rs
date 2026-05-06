use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BoardRunStatus {
    Created,
    Uploading,
    Importing,
    Completed,
    Failed,
    TimedOut,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Ready,
    NoBaseline,
    Failed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Pending,
    Ready,
    NoBaseline,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct BoardRun {
    pub id: Uuid,
    pub board_project_id: Uuid,
    pub commit_sha: String,
    pub branch: String,
    pub r#ref: String,
    pub github_run_id: i64,
    pub github_run_attempt: i32,
    pub tree_hash: Option<String>,
    pub status: BoardRunStatus,
    pub erc_status: Option<CheckStatus>,
    pub erc_errors: i32,
    pub erc_warnings: i32,
    pub drc_status: Option<CheckStatus>,
    pub drc_errors: i32,
    pub drc_warnings: i32,
    pub review_status: ReviewStatus,
    pub diff_status: DiffStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub timed_out_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
