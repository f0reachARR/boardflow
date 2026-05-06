use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};
use serde::{Deserialize, Serialize};

mod optional_public_id {
    use super::BoardProjectId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<BoardProjectId>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(id) => serializer.serialize_str(&id.to_string()),
            None => serializer.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<BoardProjectId>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse().map(Some).map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanRequest {
    pub repository: PlanRepositoryInput,
    pub git: PlanGitInput,
    pub action: PlanActionInput,
    pub mode: PlanMode,
    pub projects: Vec<PlanProjectInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanRepositoryInput {
    #[serde(default)]
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanGitInput {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub branch: String,
    pub commit_sha: String,
    pub event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanActionInput {
    pub workflow: String,
    pub run_id: String,
    pub run_attempt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    Auto,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanProjectInput {
    pub project_path: String,
    pub config_path: String,
    pub project_dir: String,
    pub tree_hash: String,
    pub files: Vec<PlanProjectFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanProjectFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanResponse {
    pub repository: PlanRepositoryOutput,
    pub projects: Vec<PlanProjectOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanRepositoryOutput {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanProjectOutput {
    pub project_path: String,
    #[serde(with = "optional_public_id")]
    pub board_project_id: Option<BoardProjectId>,
    pub decision: PlanDecision,
    pub reason: PlanReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub latest_completed_run_id: Option<BoardRunId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    Build,
    Skip,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    NewProject,
    HashChanged,
    ConfigChanged,
    ManualDispatch,
    Unchanged,
    PreviousFailed,
    NoPreviousSnapshot,
    DuplicateProjectPath,
    InvalidProjectPath,
    InvalidTreeHash,
    InvalidConfigPath,
}
