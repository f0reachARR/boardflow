use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams, encode_cursor};

use super::dto::{
    BoardProjectDetailResponse, BoardProjectListItem, RepositoryRef, derive_board_project_state,
    parse_board_run_status,
};
use crate::services::authz::{check_repo_access, ensure_repository_access};

// ─── GET /api/v1/repositories/{github_repository_id}/board-projects ──────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}/board-projects",
    params(
        ("github_repository_id" = i64, Path, description = "GitHub repository ID"),
        PaginationParams
    ),
    responses(
        (status = 200, description = "BoardProject list", body = PaginatedResponse<BoardProjectListItem>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_board_projects(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<BoardProjectListItem>>, AppError> {
    let repo = ensure_repository_access(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        github_repository_id,
        &request_id,
    )
    .await?;

    let limit = params.effective_limit();
    let cursor = params.decoded_cursor(&request_id)?;

    let rows = boardflow_db::queries::board_project::list_by_repository_id_with_status(
        &pool,
        repo.id,
        limit + 1,
        cursor,
    )
    .await
    .map_err(|e| {
        tracing::error!("list_board_projects failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|bp| {
            let state = derive_board_project_state(
                bp.latest_completed_run_id,
                bp.latest_run_status
                    .as_deref()
                    .and_then(parse_board_run_status),
            );
            BoardProjectListItem {
                board_project_id: BoardProjectId::from(bp.id),
                project_path: bp.project_path.clone(),
                project_dir: bp.project_dir.clone(),
                display_name: bp.display_name.clone(),
                state,
                latest_completed_run_id: bp.latest_completed_run_id.map(BoardRunId::from),
                latest_tree_hash: bp.latest_tree_hash.clone(),
                issue_url: bp.issue_url.clone(),
                updated_at: bp.updated_at.to_rfc3339(),
            }
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_cursor(&last.updated_at, &last.id)
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

// ─── GET /api/v1/board-projects/{board_project_id} ───────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-projects/{board_project_id}",
    params(("board_project_id" = String, Path, description = "BoardProject ID (bp_ prefix)")),
    responses(
        (status = 200, description = "BoardProject detail", body = BoardProjectDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_board_project(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_project_id): Path<String>,
) -> Result<Json<BoardProjectDetailResponse>, AppError> {
    let id = board_project_id
        .parse::<BoardProjectId>()
        .map(BoardProjectId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_project_id format", &request_id))?;

    let row = boardflow_db::queries::board_project::find_by_id_with_repository(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_project failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board project not found", &request_id))?;

    check_repo_access(
        &access_checker,
        &session.user.github_access_token,
        &row.repo_owner,
        &row.repo_name,
        "board project not found",
        &request_id,
    )
    .await?;

    // Get latest run status for state derivation
    let latest_run_status = boardflow_db::queries::board_project::get_latest_run_status(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_project latest status failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let state = derive_board_project_state(
        row.latest_completed_run_id,
        latest_run_status
            .as_deref()
            .and_then(parse_board_run_status),
    );

    Ok(Json(BoardProjectDetailResponse {
        board_project_id: BoardProjectId::from(row.id),
        repository: RepositoryRef {
            github_repository_id: row.github_repository_id.to_string(),
            owner: row.repo_owner,
            name: row.repo_name,
        },
        project_path: row.project_path,
        project_dir: row.project_dir,
        display_name: row.display_name,
        state,
        latest_completed_run_id: row.latest_completed_run_id.map(BoardRunId::from),
        latest_tree_hash: row.latest_tree_hash,
        issue_number: row.issue_number,
        issue_url: row.issue_url,
        recreate_issue_on_update: row.recreate_issue_on_update,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }))
}
