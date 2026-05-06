use boardflow_domain::public_ids::{ArtifactBundleId, BoardProjectId, BoardRunId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreateBoardRunStatus {
    Created,
    Importing,
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArtifactBundleUploadMode {
    StagingS3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ArtifactBundleUploadMethod {
    #[serde(rename = "PUT")]
    Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FailBoardRunStatus {
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ImportArtifactBundleStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBoardRunRequest {
    pub board_project_id: BoardProjectId,
    pub project_path: String,
    pub tree_hash: String,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBoardRunResponse {
    pub board_run_id: BoardRunId,
    pub status: CreateBoardRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub artifact_bundle: Option<ArtifactBundleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArtifactBundleInfo {
    pub upload_mode: ArtifactBundleUploadMode,
    pub object_key: String,
    pub upload_url: String,
    pub method: ArtifactBundleUploadMethod,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailBoardRunRequest {
    pub status: FailBoardRunStatus,
    pub error: FailErrorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailErrorInfo {
    pub message: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailBoardRunResponse {
    pub board_run_id: BoardRunId,
    pub status: FailBoardRunStatus,
    pub failed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportArtifactBundleRequest {
    pub staging_object_key: String,
    pub bundle_sha256: String,
    pub bundle_size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportArtifactBundleResponse {
    pub bundle_id: ArtifactBundleId,
    pub status: ImportArtifactBundleStatus,
}
