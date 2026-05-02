pub mod create_dashboard_comment;
pub mod create_issue;
pub mod create_run_result_comment;
pub mod import;
pub mod update_dashboard_comment;

use boardflow_db::queries::board_run;
use sqlx::PgPool;
use uuid::Uuid;

/// Result of a job handler execution.
pub enum HandlerResult {
    /// Job completed successfully.
    Completed,
    /// Job should be rescheduled for retry.
    Reschedule { reason: String, backoff_secs: f64 },
    /// Job failed terminally.
    Failed { reason: String },
}

/// Check whether the tree_hash has changed between the current run and the previous completed run.
/// Returns `Ok(true)` if tree_hash changed (or no previous run exists), `Ok(false)` if unchanged.
pub async fn tree_hash_changed(
    pool: &PgPool,
    board_project_id: Uuid,
    current_run_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let current_run = board_run::find_by_id(pool, current_run_id).await?;
    let current_tree_hash = current_run.and_then(|r| r.tree_hash);

    let prev_run =
        board_run::find_previous_completed(pool, board_project_id, current_run_id).await?;
    let prev_tree_hash = prev_run.and_then(|r| r.tree_hash);

    // If no previous run exists, consider it changed (first run)
    if prev_tree_hash.is_none() {
        return Ok(true);
    }

    Ok(current_tree_hash != prev_tree_hash)
}
