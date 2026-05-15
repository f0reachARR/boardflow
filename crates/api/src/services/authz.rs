use sqlx::PgPool;
use uuid::Uuid;

use boardflow_domain::models::repository::Repository;

use crate::error::AppError;
use crate::github_access::{DynGithubAccessChecker, access_result_to_error};

/// Verify GitHub access for a repository identified by its `github_repository_id`.
///
/// Returns the `Repository` row on success.
pub async fn ensure_repository_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo = boardflow_db::queries::repository::find_by_github_id(pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("ensure_repository_access repo lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", request_id))?;

    check_repo_access(
        access_checker,
        github_access_token,
        &repo.owner,
        &repo.name,
        "repository not found",
        request_id,
    )
    .await?;

    Ok(repo)
}

/// Verify GitHub access for the repository that owns a given board run.
///
/// Returns the `Repository` row on success.
pub async fn ensure_board_run_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    board_run_id: Uuid,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo =
        boardflow_db::queries::board_run::find_repository_by_board_run_id(pool, board_run_id)
            .await
            .map_err(|e| {
                tracing::error!("ensure_board_run_access repo lookup failed: {e}");
                AppError::internal_error("database error", request_id)
            })?
            .ok_or_else(|| AppError::not_found("board run not found", request_id))?;

    check_repo_access(
        access_checker,
        github_access_token,
        &repo.owner,
        &repo.name,
        "board run not found",
        request_id,
    )
    .await?;

    Ok(repo)
}

/// Verify GitHub access for the repository that owns a given board project.
///
/// Returns the `Repository` row on success.
pub async fn ensure_board_project_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    board_project_id: Uuid,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo = boardflow_db::queries::board_project::find_repository_by_board_project_id(
        pool,
        board_project_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("ensure_board_project_access repo lookup failed: {e}");
        AppError::internal_error("database error", request_id)
    })?
    .ok_or_else(|| AppError::not_found("board project not found", request_id))?;

    check_repo_access(
        access_checker,
        github_access_token,
        &repo.owner,
        &repo.name,
        "board project not found",
        request_id,
    )
    .await?;

    Ok(repo)
}

/// Low-level access check: call `check_access` and convert the result.
///
/// Returns `Ok(())` if access is allowed, or an `AppError` mapping
/// `Denied` → `not_found` (security: do not reveal repo existence).
pub async fn check_repo_access(
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    owner: &str,
    name: &str,
    not_found_msg: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let result = access_checker
        .check_access(github_access_token, owner, name)
        .await;
    if let Some(err) = access_result_to_error(&result, not_found_msg, request_id) {
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::error::ErrorCode;
    use crate::github_access::{
        AllowAllGithubAccessChecker, DenyAllGithubAccessChecker, DynGithubAccessChecker,
        RateLimitedGithubAccessChecker, TokenExpiredGithubAccessChecker,
        UpstreamErrorGithubAccessChecker,
    };

    use super::check_repo_access;

    /// Allowed → Ok(())
    #[tokio::test]
    async fn check_repo_access_allowed() {
        let checker: DynGithubAccessChecker = Arc::new(AllowAllGithubAccessChecker);
        let result =
            check_repo_access(&checker, "token", "owner", "repo", "not found", "req-1").await;
        assert!(result.is_ok());
    }

    /// Denied → not_found with the caller-supplied message (security: hide repo existence)
    #[tokio::test]
    async fn check_repo_access_denied_returns_not_found() {
        let checker: DynGithubAccessChecker = Arc::new(DenyAllGithubAccessChecker);
        let err = check_repo_access(
            &checker,
            "token",
            "owner",
            "repo",
            "board run not found",
            "req-2",
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        assert_eq!(err.message, "board run not found");
    }

    /// TokenExpired → 401 unauthorized
    #[tokio::test]
    async fn check_repo_access_token_expired() {
        let checker: DynGithubAccessChecker = Arc::new(TokenExpiredGithubAccessChecker);
        let err = check_repo_access(&checker, "token", "owner", "repo", "not found", "req-3")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Unauthorized);
    }

    /// RateLimited → 429
    #[tokio::test]
    async fn check_repo_access_rate_limited() {
        let checker: DynGithubAccessChecker = Arc::new(RateLimitedGithubAccessChecker);
        let err = check_repo_access(&checker, "token", "owner", "repo", "not found", "req-4")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::RateLimited);
    }

    /// UpstreamError → 500
    #[tokio::test]
    async fn check_repo_access_upstream_error() {
        let checker: DynGithubAccessChecker = Arc::new(UpstreamErrorGithubAccessChecker);
        let err = check_repo_access(&checker, "token", "owner", "repo", "not found", "req-5")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InternalError);
    }
}
