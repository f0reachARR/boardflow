use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use boardflow_domain::models::artifact::ArtifactStatus;
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams, encode_cursor};

use super::dto::{ArtifactSummary, BoardRunDetailResponse, BoardRunListItem, CheckInfo};
use crate::services::authz::{ensure_board_project_access, ensure_board_run_access};

// ─── GET /api/v1/board-projects/{board_project_id}/board-runs ────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-projects/{board_project_id}/board-runs",
    params(
        ("board_project_id" = String, Path, description = "BoardProject ID (bp_ prefix)"),
        PaginationParams
    ),
    responses(
        (status = 200, description = "BoardRun list", body = PaginatedResponse<BoardRunListItem>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_board_runs(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_project_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<BoardRunListItem>>, AppError> {
    let bp_id = board_project_id
        .parse::<BoardProjectId>()
        .map(BoardProjectId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_project_id format", &request_id))?;

    // Verify board_project exists and check repository access
    ensure_board_project_access(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        bp_id,
        &request_id,
    )
    .await?;

    let limit = params.effective_limit();
    let cursor = params.decoded_cursor(&request_id)?;

    let rows =
        boardflow_db::queries::board_run::list_by_board_project(&pool, bp_id, limit + 1, cursor)
            .await
            .map_err(|e| {
                tracing::error!("list_board_runs failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|r| BoardRunListItem {
            board_run_id: BoardRunId::from(r.id),
            status: r.status,
            commit_sha: r.commit_sha.clone(),
            branch: r.branch.clone(),
            ref_: r.r#ref.clone(),
            github_run_id: r.github_run_id.to_string(),
            github_run_attempt: r.github_run_attempt.to_string(),
            tree_hash: r.tree_hash.clone(),
            erc_status: r.erc_status,
            erc_errors: r.erc_errors,
            erc_warnings: r.erc_warnings,
            drc_status: r.drc_status,
            drc_errors: r.drc_errors,
            drc_warnings: r.drc_warnings,
            created_at: r.created_at.to_rfc3339(),
            completed_at: r.completed_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_cursor(&last.created_at, &last.id)
        })
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    }))
}

// ─── GET /api/v1/board-runs/{board_run_id} ───────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}",
    params(("board_run_id" = String, Path, description = "BoardRun ID (br_ prefix)")),
    responses(
        (status = 200, description = "BoardRun detail", body = BoardRunDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_board_run(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_run_id): Path<String>,
) -> Result<Json<BoardRunDetailResponse>, AppError> {
    let id = board_run_id
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    ensure_board_run_access(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        id,
        &request_id,
    )
    .await?;

    let run = boardflow_db::queries::board_run::find_by_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let checks = boardflow_db::queries::run_check::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run checks failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let artifacts = boardflow_db::queries::artifact::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run artifacts failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let check_infos: Vec<CheckInfo> = checks
        .iter()
        .map(|c| CheckInfo {
            kind: c.check_kind,
            status: c.status,
            error_count: c.error_count,
            warning_count: c.warning_count,
            notice_count: c.notice_count,
        })
        .collect();

    let artifact_summary = ArtifactSummary {
        available: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Available)
            .count() as i64,
        missing: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Missing)
            .count() as i64,
        failed: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Failed)
            .count() as i64,
        skipped: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Skipped)
            .count() as i64,
    };

    Ok(Json(BoardRunDetailResponse {
        board_run_id: BoardRunId::from(run.id),
        board_project_id: BoardProjectId::from(run.board_project_id),
        status: run.status,
        commit_sha: run.commit_sha,
        branch: run.branch,
        ref_: run.r#ref,
        github_run_id: run.github_run_id.to_string(),
        github_run_attempt: run.github_run_attempt.to_string(),
        tree_hash: run.tree_hash,
        checks: check_infos,
        artifact_summary,
        created_at: run.created_at.to_rfc3339(),
        completed_at: run.completed_at.map(|t| t.to_rfc3339()),
    }))
}
