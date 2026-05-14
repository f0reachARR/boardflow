use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use boardflow_domain::models::artifact::{ArtifactStatus, ArtifactType};
use boardflow_domain::models::board_run::{BoardRunStatus, CheckStatus};
use boardflow_domain::models::run_check::{CheckKind, RunCheckStatus};
use boardflow_domain::models::snapshot::BoardRunDiffStatus;
use boardflow_domain::public_ids::{ArtifactId, BoardProjectId, BoardRunId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardProjectState {
    Detected,
    Processing,
    Failed,
    TimedOut,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewerAvailabilityStatus {
    Available,
    Partial,
    Skipped,
    Failed,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewerSourceKind {
    Project,
    Schematic,
    Board,
}

pub(crate) fn parse_board_run_status(status: &str) -> Option<BoardRunStatus> {
    match status {
        "created" => Some(BoardRunStatus::Created),
        "uploading" => Some(BoardRunStatus::Uploading),
        "importing" => Some(BoardRunStatus::Importing),
        "completed" => Some(BoardRunStatus::Completed),
        "failed" => Some(BoardRunStatus::Failed),
        "timed_out" => Some(BoardRunStatus::TimedOut),
        _ => None,
    }
}

// ─── Response types ──────────────────────────────────────────────────────────

// Repository responses
#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryListItem {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
    pub installation_id: String,
    pub board_project_count: i64,
    pub latest_run_status: Option<BoardRunStatus>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryDetailResponse {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
    pub installation_id: String,
    pub html_url: String,
    pub board_project_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

// BoardProject responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardProjectListItem {
    pub board_project_id: BoardProjectId,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub state: BoardProjectState,
    pub latest_completed_run_id: Option<BoardRunId>,
    pub latest_tree_hash: Option<String>,
    pub issue_url: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BoardProjectDetailResponse {
    pub board_project_id: BoardProjectId,
    pub repository: RepositoryRef,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub state: BoardProjectState,
    pub latest_completed_run_id: Option<BoardRunId>,
    pub latest_tree_hash: Option<String>,
    pub issue_number: Option<i32>,
    pub issue_url: Option<String>,
    pub recreate_issue_on_update: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryRef {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

// BoardRun responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunListItem {
    pub board_run_id: BoardRunId,
    pub status: BoardRunStatus,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
    pub tree_hash: Option<String>,
    pub erc_status: Option<CheckStatus>,
    pub erc_errors: i32,
    pub erc_warnings: i32,
    pub drc_status: Option<CheckStatus>,
    pub drc_errors: i32,
    pub drc_warnings: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunDetailResponse {
    pub board_run_id: BoardRunId,
    pub board_project_id: BoardProjectId,
    pub status: BoardRunStatus,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
    pub tree_hash: Option<String>,
    pub checks: Vec<CheckInfo>,
    pub artifact_summary: ArtifactSummary,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckInfo {
    pub kind: CheckKind,
    pub status: RunCheckStatus,
    pub error_count: i32,
    pub warning_count: i32,
    pub notice_count: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactSummary {
    pub available: i64,
    pub missing: i64,
    pub failed: i64,
    pub skipped: i64,
}

// Artifact responses
#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactListItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<ArtifactId>,
    pub r#type: ArtifactType,
    pub status: ArtifactStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

// Viewer Sources responses
#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerSourcesResponse {
    pub board_run_id: BoardRunId,
    pub expires_at: String,
    pub viewers: ViewerMap,
}

// Diff responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunDiffResponse {
    pub board_run_id: BoardRunId,
    pub base_board_run_id: Option<BoardRunId>,
    pub status: BoardRunDiffStatus,
    pub summary: Option<serde_json::Value>,
    pub metadata: Option<DiffMetadataResponse>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiffMetadataResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hashes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bom_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previews: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerMap {
    pub kicanvas: ViewerStatus,
    pub schematic: ViewerStatus,
    pub pcb_preview: ViewerStatus,
    pub ibom: ViewerStatus,
    pub bom: ViewerStatus,
    pub fabrication: ViewerStatus,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerStatus {
    pub status: ViewerAvailabilityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<ViewerSource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<ViewerSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iframe_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<Vec<ViewerDownload>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<ArtifactId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<ArtifactType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ViewerSourceKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerDownload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<ArtifactId>,
    pub artifact_type: ArtifactType,
    pub status: ArtifactStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

// ─── Helper: board project state derivation ──────────────────────────────────

pub(crate) fn derive_board_project_state(
    latest_completed_run_id: Option<Uuid>,
    latest_run_status: Option<BoardRunStatus>,
) -> BoardProjectState {
    match latest_completed_run_id {
        Some(_) => BoardProjectState::Completed,
        None => match latest_run_status {
            Some(BoardRunStatus::Failed) => BoardProjectState::Failed,
            Some(BoardRunStatus::TimedOut) => BoardProjectState::TimedOut,
            Some(
                BoardRunStatus::Created | BoardRunStatus::Uploading | BoardRunStatus::Importing,
            ) => BoardProjectState::Processing,
            _ => BoardProjectState::Detected,
        },
    }
}
