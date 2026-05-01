use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum CheckKind {
    Erc,
    Drc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum RunCheckStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct RunCheck {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub check_kind: CheckKind,
    pub tool_name: Option<String>,
    pub tool_version: Option<String>,
    pub status: RunCheckStatus,
    pub error_count: i32,
    pub warning_count: i32,
    pub notice_count: i32,
    pub report_artifact_id: Option<Uuid>,
    pub raw_summary_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Warning,
    Notice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum SubjectKind {
    Schematic,
    Pcb,
    Net,
    Footprint,
    Symbol,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct RunCheckFinding {
    pub id: Uuid,
    pub run_check_id: Uuid,
    pub severity: FindingSeverity,
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<SubjectKind>,
    pub subject_ref: Option<String>,
    pub sheet_path: Option<String>,
    pub pcb_layer: Option<String>,
    pub x_um: Option<i32>,
    pub y_um: Option<i32>,
    pub bbox_json: Option<serde_json::Value>,
    pub raw_payload_json: Option<serde_json::Value>,
    pub sort_index: i32,
    pub created_at: DateTime<Utc>,
}

/// List row for findings API — excludes bbox_json and raw_payload_json for bandwidth savings.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunCheckFindingListRow {
    pub id: Uuid,
    pub run_check_id: Uuid,
    pub severity: FindingSeverity,
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<SubjectKind>,
    pub subject_ref: Option<String>,
    pub sheet_path: Option<String>,
    pub pcb_layer: Option<String>,
    pub x_um: Option<i32>,
    pub y_um: Option<i32>,
    pub sort_index: i32,
    pub created_at: DateTime<Utc>,
}
