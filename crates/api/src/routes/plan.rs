use axum::extract::State;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedToken;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanRequest {
    pub repository: PlanRepositoryInput,
    pub git: PlanGitInput,
    pub action: PlanActionInput,
    pub mode: PlanMode,
    pub projects: Vec<PlanProjectInput>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanRepositoryInput {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanGitInput {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub branch: String,
    pub commit_sha: String,
    pub event_name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanActionInput {
    pub workflow: String,
    pub run_id: String,
    pub run_attempt: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    Auto,
    All,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanProjectInput {
    pub project_path: String,
    pub config_path: String,
    pub project_dir: String,
    pub tree_hash: String,
    pub files: Vec<PlanProjectFile>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanProjectFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanResponse {
    pub repository: PlanRepositoryOutput,
    pub projects: Vec<PlanProjectOutput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanRepositoryOutput {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanProjectOutput {
    pub project_path: String,
    pub board_project_id: String,
    pub decision: PlanDecision,
    pub reason: PlanReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_completed_run_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    Build,
    Skip,
    Error,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    NewProject,
    HashChanged,
    ConfigChanged,
    ManualDispatch,
    Unchanged,
    PreviousFailed,
    NoPreviousSnapshot,
}

#[utoipa::path(
    post,
    path = "/api/v1/runs/plan",
    request_body = PlanRequest,
    responses(
        (status = 200, description = "Plan decisions", body = PlanResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn plan_run(
    auth: AuthenticatedToken,
    Extension(request_id): Extension<RequestId>,
    State(pool): State<PgPool>,
    Json(req): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, AppError> {
    let rid = &request_id.0;

    // 1. Parse github_repository_id
    let github_repository_id: i64 = req
        .repository
        .github_repository_id
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_repository_id", rid))?;

    // 2. Repository upsert
    let repo = boardflow_db::queries::repository::upsert(
        &pool,
        github_repository_id,
        &req.repository.owner,
        &req.repository.name,
        auth.0.installation_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("repository upsert failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    // 3. Authorization: token's repository_id must match upserted repo
    if repo.id != auth.0.repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this repository",
            rid,
        ));
    }

    // 4. Process each project
    let mut project_outputs = Vec::with_capacity(req.projects.len());
    for project in &req.projects {
        let display_name = project
            .project_path
            .rsplit('/')
            .next()
            .unwrap_or(&project.project_path)
            .strip_suffix(".kicad_pro")
            .unwrap_or(
                project
                    .project_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&project.project_path),
            )
            .to_string();

        let bp = boardflow_db::queries::board_project::upsert(
            &pool,
            repo.id,
            &project.project_path,
            &project.project_dir,
            &display_name,
        )
        .await
        .map_err(|e| {
            tracing::error!("board_project upsert failed: {e}");
            AppError::internal_error("database error", rid)
        })?;

        let (decision, reason) = match req.mode {
            PlanMode::All => (PlanDecision::Build, PlanReason::ManualDispatch),
            PlanMode::Auto => match &bp.latest_tree_hash {
                None => (PlanDecision::Build, PlanReason::NoPreviousSnapshot),
                Some(hash) if hash != &project.tree_hash => {
                    (PlanDecision::Build, PlanReason::HashChanged)
                }
                Some(_) => (PlanDecision::Skip, PlanReason::Unchanged),
            },
        };

        project_outputs.push(PlanProjectOutput {
            project_path: project.project_path.clone(),
            board_project_id: format!("bp_{}", bp.id),
            decision,
            reason,
            latest_completed_run_id: bp.latest_completed_run_id.map(|id| format!("br_{}", id)),
        });
    }

    // 5. Return response
    Ok(Json(PlanResponse {
        repository: PlanRepositoryOutput {
            github_repository_id: req.repository.github_repository_id,
            owner: req.repository.owner,
            name: req.repository.name,
        },
        projects: project_outputs,
    }))
}
