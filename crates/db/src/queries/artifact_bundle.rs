use boardflow_domain::models::artifact_bundle::ArtifactBundle;
use uuid::Uuid;

/// Find ArtifactBundle by board_run_id
pub async fn find_by_board_run_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
) -> Result<Option<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        "SELECT * FROM artifact_bundles WHERE board_run_id = $1 ORDER BY received_at DESC LIMIT 1",
    )
    .bind(board_run_id)
    .fetch_optional(executor)
    .await
}

/// Insert a new ArtifactBundle (staging_s3 mode)
pub async fn insert_staging(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    staging_object_key: &str,
) -> Result<ArtifactBundle, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        r#"INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, status, received_at)
        VALUES ($1, $2, 'staging_s3', $3, 'pending', NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_run_id)
    .bind(staging_object_key)
    .fetch_one(executor)
    .await
}

/// Update ArtifactBundle with sha256 and size_bytes, set status to 'pending' (import queued).
/// Only updates if sha256 is currently NULL and staging_object_key matches.
/// Returns None if conditions not met (another request already set sha256).
pub async fn update_for_import(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    staging_object_key: &str,
    sha256: &str,
    size_bytes: i64,
) -> Result<Option<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        r#"UPDATE artifact_bundles SET sha256 = $3, size_bytes = $4, status = 'pending'
        WHERE id = $1 AND staging_object_key = $2 AND sha256 IS NULL
        RETURNING *"#,
    )
    .bind(id)
    .bind(staging_object_key)
    .bind(sha256)
    .bind(size_bytes)
    .fetch_optional(executor)
    .await
}

/// Find ArtifactBundle by board_run_id + staging_object_key + sha256 (idempotency check)
pub async fn find_by_import_key(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
    staging_object_key: &str,
    sha256: &str,
) -> Result<Option<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        "SELECT * FROM artifact_bundles WHERE board_run_id = $1 AND staging_object_key = $2 AND sha256 = $3",
    )
    .bind(board_run_id)
    .bind(staging_object_key)
    .bind(sha256)
    .fetch_optional(executor)
    .await
}

/// Find any existing bundle for the run (for conflict check)
pub async fn find_existing_for_run(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
) -> Result<Option<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        "SELECT * FROM artifact_bundles WHERE board_run_id = $1 AND sha256 IS NOT NULL LIMIT 1",
    )
    .bind(board_run_id)
    .fetch_optional(executor)
    .await
}

/// Mark bundle as importing
pub async fn mark_importing(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE artifact_bundles SET status = 'importing' WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Mark bundle as completed
pub async fn mark_completed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE artifact_bundles SET status = 'completed', validated_at = NOW(), delete_after = NOW() + INTERVAL '24 hours' WHERE id = $1",
    )
    .bind(id)
    .execute(executor)
    .await?;
    Ok(())
}

/// Mark bundle as failed (delete_after = 7 days per spec)
pub async fn mark_failed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE artifact_bundles SET status = 'failed', error_message = $2, delete_after = NOW() + INTERVAL '7 days' WHERE id = $1",
    )
    .bind(id)
    .bind(error_message)
    .execute(executor)
    .await?;
    Ok(())
}

/// Find expired staging bundles (delete_after < NOW() and staging_object_key IS NOT NULL).
/// Returns up to 100 bundles per sweep cycle, ordered by oldest first for deterministic processing.
pub async fn find_expired_staging(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<Vec<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        "SELECT * FROM artifact_bundles WHERE delete_after < NOW() AND staging_object_key IS NOT NULL ORDER BY delete_after ASC, id ASC LIMIT 100",
    )
    .fetch_all(executor)
    .await
}

/// Clear staging_object_key after successful S3 deletion.
pub async fn clear_staging_object_key(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE artifact_bundles SET staging_object_key = NULL WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Set delete_after for staging bundles belonging to timed-out runs.
/// Called after sweep_timed_out marks runs as timed_out.
pub async fn set_delete_after_for_timed_out_runs(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    run_ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if run_ids.is_empty() {
        return Ok(0);
    }
    let result = sqlx::query(
        r#"UPDATE artifact_bundles
        SET delete_after = NOW() + INTERVAL '7 days'
        WHERE board_run_id = ANY($1)
        AND staging_object_key IS NOT NULL
        AND delete_after IS NULL"#,
    )
    .bind(run_ids)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// Repair orphaned staging bundles: set delete_after for bundles belonging to
/// terminal runs (timed_out or failed) where delete_after is not yet set.
/// This provides self-healing in case set_delete_after_for_timed_out_runs failed.
/// Uses timed_out_at/created_at as base to preserve the spec's "7 days from event" semantics.
pub async fn repair_orphaned_staging_bundles(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE artifact_bundles ab
        SET delete_after = GREATEST(
            COALESCE(br.timed_out_at, br.created_at) + INTERVAL '7 days',
            NOW()
        )
        FROM board_runs br
        WHERE ab.board_run_id = br.id
        AND br.status IN ('timed_out', 'failed')
        AND ab.staging_object_key IS NOT NULL
        AND ab.delete_after IS NULL"#,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}
