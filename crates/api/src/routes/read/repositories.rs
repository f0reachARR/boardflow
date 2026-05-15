use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams, encode_repository_cursor};

use super::dto::{RepositoryDetailResponse, RepositoryListItem, parse_board_run_status};
use crate::github_access::{access_error_to_app_error, access_result_to_error};

// ─── GET /api/v1/repositories ────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories",
    params(PaginationParams),
    responses(
        (status = 200, description = "Repository list", body = PaginatedResponse<RepositoryListItem>),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_repositories(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<RepositoryListItem>>, AppError> {
    let limit = params.effective_limit();
    let cursor = params.decoded_repository_cursor(&request_id)?;

    // Pre-filter: get accessible repo ids from GitHub API
    let token = &session.user.github_access_token;
    let accessible_ids = access_checker
        .list_accessible_repo_ids(token)
        .await
        .map_err(|e| access_error_to_app_error(&e, &request_id))?;

    let rows = boardflow_db::queries::repository::list_with_stats(
        &pool,
        limit + 1,
        cursor,
        accessible_ids.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("list_repositories failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|r| RepositoryListItem {
            github_repository_id: r.github_repository_id.to_string(),
            owner: r.owner.clone(),
            name: r.name.clone(),
            installation_id: r.installation_id.to_string(),
            board_project_count: r.board_project_count,
            latest_run_status: r
                .latest_run_status
                .as_deref()
                .and_then(parse_board_run_status),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_repository_cursor(&last.updated_at, last.github_repository_id)
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

// ─── GET /api/v1/repositories/{github_repository_id} ─────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}",
    params(("github_repository_id" = i64, Path, description = "GitHub repository ID")),
    responses(
        (status = 200, description = "Repository detail", body = RepositoryDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_repository(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
) -> Result<Json<RepositoryDetailResponse>, AppError> {
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("get_repository failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    let board_project_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM board_projects WHERE repository_id = $1")
            .bind(repo.id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                tracing::error!("count board_projects failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    let html_url = format!("https://github.com/{}/{}", repo.owner, repo.name);

    Ok(Json(RepositoryDetailResponse {
        github_repository_id: repo.github_repository_id.to_string(),
        owner: repo.owner,
        name: repo.name,
        installation_id: repo.installation_id.to_string(),
        html_url,
        board_project_count,
        created_at: repo.created_at.to_rfc3339(),
        updated_at: repo.updated_at.to_rfc3339(),
    }))
}
