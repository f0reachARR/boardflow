use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use boardflow_api_types::plan::*;
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};

use crate::error::AppError;

pub(crate) async fn execute_plan_run(
    pool: &PgPool,
    token_repository_id: Uuid,
    installation_id: i64,
    req: PlanRequest,
    request_id: &str,
) -> Result<PlanResponse, AppError> {
    // 1. Parse github_repository_id
    let github_repository_id: i64 = req
        .repository
        .github_repository_id
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_repository_id", request_id))?;

    // 2. Authorization: verify token's repository matches the request's github_repository_id
    let existing_repo = boardflow_db::queries::repository::find_by_id(pool, token_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("repository lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| {
            tracing::error!(
                "token references non-existent repository_id={}",
                token_repository_id
            );
            AppError::internal_error("token references invalid repository", request_id)
        })?;

    if existing_repo.github_repository_id != github_repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this repository",
            request_id,
        ));
    }

    // 3. Repository upsert (owner/name update only, auth already verified)
    let repo = boardflow_db::queries::repository::upsert(
        pool,
        github_repository_id,
        &req.repository.owner,
        &req.repository.name,
        installation_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("repository upsert failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    // 4. Validate projects: detect duplicates and invalid paths
    let mut seen_paths: HashSet<&str> = HashSet::new();
    let mut duplicate_paths: HashSet<&str> = HashSet::new();
    for project in &req.projects {
        if !seen_paths.insert(&project.project_path) {
            duplicate_paths.insert(&project.project_path);
        }
    }

    // 5. Process each project
    let mut project_outputs = Vec::with_capacity(req.projects.len());
    for project in &req.projects {
        // Validation: empty or malformed project_path
        if project.project_path.is_empty()
            || project.project_path.starts_with('/')
            || project.project_path.contains("..")
            || !project.project_path.ends_with(".kicad_pro")
        {
            project_outputs.push(PlanProjectOutput {
                project_path: project.project_path.clone(),
                board_project_id: None,
                decision: PlanDecision::Error,
                reason: PlanReason::InvalidProjectPath,
                latest_completed_run_id: None,
            });
            continue;
        }

        // Validation: duplicate project_path
        if duplicate_paths.contains(project.project_path.as_str()) {
            project_outputs.push(PlanProjectOutput {
                project_path: project.project_path.clone(),
                board_project_id: None,
                decision: PlanDecision::Error,
                reason: PlanReason::DuplicateProjectPath,
                latest_completed_run_id: None,
            });
            continue;
        }

        // Validation: empty or malformed tree_hash
        if project.tree_hash.is_empty() || project.tree_hash.chars().any(|c| c.is_whitespace()) {
            project_outputs.push(PlanProjectOutput {
                project_path: project.project_path.clone(),
                board_project_id: None,
                decision: PlanDecision::Error,
                reason: PlanReason::InvalidTreeHash,
                latest_completed_run_id: None,
            });
            continue;
        }

        // Validation: empty or malformed config_path
        if project.config_path.is_empty()
            || project.config_path.starts_with('/')
            || project.config_path.contains("..")
        {
            project_outputs.push(PlanProjectOutput {
                project_path: project.project_path.clone(),
                board_project_id: None,
                decision: PlanDecision::Error,
                reason: PlanReason::InvalidConfigPath,
                latest_completed_run_id: None,
            });
            continue;
        }

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
            pool,
            repo.id,
            &project.project_path,
            &project.project_dir,
            &display_name,
        )
        .await
        .map_err(|e| {
            tracing::error!("board_project upsert failed: {e}");
            AppError::internal_error("database error", request_id)
        })?;

        let is_new = bp.created_at == bp.updated_at;
        let (decision, reason) = match req.mode {
            PlanMode::All => (PlanDecision::Build, PlanReason::ManualDispatch),
            PlanMode::Auto => {
                if is_new {
                    (PlanDecision::Build, PlanReason::NewProject)
                } else {
                    match &bp.latest_tree_hash {
                        None => (PlanDecision::Build, PlanReason::NoPreviousSnapshot),
                        Some(hash) if hash != &project.tree_hash => {
                            (PlanDecision::Build, PlanReason::HashChanged)
                        }
                        Some(_) => (PlanDecision::Skip, PlanReason::Unchanged),
                    }
                }
            }
        };

        project_outputs.push(PlanProjectOutput {
            project_path: project.project_path.clone(),
            board_project_id: Some(BoardProjectId::from(bp.id)),
            decision,
            reason,
            latest_completed_run_id: bp.latest_completed_run_id.map(BoardRunId::from),
        });
    }

    // 6. Return response
    Ok(PlanResponse {
        repository: PlanRepositoryOutput {
            github_repository_id: req.repository.github_repository_id,
            owner: req.repository.owner,
            name: req.repository.name,
        },
        projects: project_outputs,
    })
}
