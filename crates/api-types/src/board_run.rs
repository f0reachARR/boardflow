use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBoardRunRequest {
    pub board_project_id: String,
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
    pub board_run_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub artifact_bundle: Option<ArtifactBundleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArtifactBundleInfo {
    pub upload_mode: String,
    pub object_key: String,
    pub upload_url: String,
    pub method: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailBoardRunRequest {
    pub status: String,
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
    pub board_run_id: String,
    pub status: String,
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
    pub bundle_id: String,
    pub status: String,
}
