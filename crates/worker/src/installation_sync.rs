//! Issue #105: Worker-side reconciliation of the `repositories` table for
//! installations whose webhook stream is stale.
//!
//! The repository list API serves data from the local `repositories` table.
//! Webhook events are the primary source of truth for which repositories belong
//! to which installation; the worker here is the safety net for missed webhooks
//! and for the first-time view of an installation we have never seen.

use std::collections::HashSet;
use std::time::Duration as StdDuration;

use boardflow_db::queries::{github_installation_sync_state, repository};
use boardflow_github::{GitHubAppClient, GitHubClientError, InstallationRepoInfo};
use chrono::Duration as ChronoDuration;
use sqlx::PgPool;

use crate::config::WorkerConfig;

/// Single sweep: reconcile installations that are due. Safe to call repeatedly.
pub async fn sync_stale_installations(
    pool: &PgPool,
    config: &WorkerConfig,
    github_client: &dyn GitHubAppClient,
) {
    let max_per_sweep = config.installation_sync_max_per_sweep as usize;
    if max_per_sweep == 0 {
        return;
    }

    let installation_ids = match github_client.list_installation_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(error = %e, "installation sync: failed to list app installations");
            return;
        }
    };

    if installation_ids.is_empty() {
        tracing::debug!("installation sync: no installations to reconcile");
        return;
    }

    let stale_after = ChronoDuration::seconds(config.installation_sync_stale_after_secs as i64);
    let min_interval = ChronoDuration::seconds(config.installation_sync_min_interval_secs as i64);

    let mut synced = 0usize;
    for installation_id in installation_ids {
        if synced >= max_per_sweep {
            tracing::info!(
                limit = max_per_sweep,
                "installation sync: reached per-sweep cap, deferring remaining installations"
            );
            break;
        }

        let installation_id_i64 = installation_id as i64;
        let state = match github_installation_sync_state::find_by_installation_id(
            pool,
            installation_id_i64,
        )
        .await
        {
            Ok(state) => state,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    installation_id,
                    "installation sync: failed to read sync state, skipping"
                );
                continue;
            }
        };

        if !needs_sync(state.as_ref(), stale_after, min_interval) {
            continue;
        }

        if let Err(e) =
            github_installation_sync_state::mark_sync_started(pool, installation_id_i64).await
        {
            tracing::warn!(
                error = %e,
                installation_id,
                "installation sync: failed to record sync_started, skipping"
            );
            continue;
        }

        let outcome = reconcile_installation(pool, github_client, installation_id).await;
        let (status, err) = match &outcome {
            Ok(_) => ("success", None),
            Err(e) => ("failed", Some(e.clone())),
        };

        if let Err(e) = github_installation_sync_state::mark_sync_completed(
            pool,
            installation_id_i64,
            status,
            err.as_deref(),
        )
        .await
        {
            tracing::warn!(
                error = %e,
                installation_id,
                "installation sync: failed to record sync_completed"
            );
        }

        match outcome {
            Ok(repo_count) => {
                tracing::info!(
                    installation_id,
                    repo_count,
                    "installation sync: reconciled installation"
                );
            }
            Err(e) => {
                tracing::warn!(
                    installation_id,
                    error = %e,
                    "installation sync: failed to reconcile installation"
                );
            }
        }
        synced += 1;
    }
}

fn needs_sync(
    state: Option<&github_installation_sync_state::InstallationSyncState>,
    stale_after: ChronoDuration,
    min_interval: ChronoDuration,
) -> bool {
    let now = chrono::Utc::now();

    let state = match state {
        None => return true,
        Some(s) => s,
    };

    // Throttle: don't double-sync if we just ran (or just started).
    if let Some(started) = state.last_sync_started_at {
        if now - started < min_interval {
            return false;
        }
    }
    if let Some(completed) = state.last_sync_completed_at {
        if now - completed < min_interval {
            // Recently completed; only re-sync if previous attempt failed AND throttle window
            // already passed (handled above).
            return state.last_sync_status.as_deref() == Some("failed")
                && now - completed >= min_interval;
        }
    }

    // Eligibility: either no webhook ever, webhook old enough, or last sync failed.
    let webhook_stale = match state.webhook_seen_at {
        None => true,
        Some(seen) => now - seen >= stale_after,
    };
    let failed = state.last_sync_status.as_deref() == Some("failed");

    webhook_stale || failed
}

async fn reconcile_installation(
    pool: &PgPool,
    github_client: &dyn GitHubAppClient,
    installation_id: u64,
) -> Result<usize, String> {
    let repos = match github_client
        .list_installation_repositories(installation_id)
        .await
    {
        Ok(repos) => repos,
        Err(GitHubClientError::NotFound(msg)) => {
            // Installation no longer accessible (deleted/suspended). Treat as
            // empty so we clear stale rows.
            tracing::warn!(installation_id, error = %msg, "installation not found, clearing rows");
            Vec::new()
        }
        Err(e) => return Err(e.to_string()),
    };

    upsert_and_prune(pool, installation_id as i64, &repos)
        .await
        .map_err(|e| format!("db error: {e}"))
}

async fn upsert_and_prune(
    pool: &PgPool,
    installation_id: i64,
    repos: &[InstallationRepoInfo],
) -> Result<usize, sqlx::Error> {
    let mut keep: HashSet<i64> = HashSet::with_capacity(repos.len());

    for repo in repos {
        repository::upsert(pool, repo.id, &repo.owner, &repo.name, installation_id).await?;
        keep.insert(repo.id);
    }

    // Anything still claimed by this installation but not in the latest set
    // has been removed — drop the installation linkage.
    let current_repos = repository::list_github_ids_for_installation(pool, installation_id).await?;
    for github_id in current_repos {
        if !keep.contains(&github_id) {
            let _ =
                repository::clear_installation_for_repo(pool, github_id, installation_id).await?;
        }
    }

    Ok(repos.len())
}

/// Convenience wrapper used by the dispatcher loop.
pub fn interval(config: &WorkerConfig) -> tokio::time::Interval {
    tokio::time::interval(StdDuration::from_secs(
        config.installation_sync_interval_secs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use boardflow_db::queries::github_installation_sync_state::InstallationSyncState;
    use chrono::Utc;

    fn state(
        webhook_seen_at: Option<chrono::DateTime<Utc>>,
        last_sync_started_at: Option<chrono::DateTime<Utc>>,
        last_sync_completed_at: Option<chrono::DateTime<Utc>>,
        last_sync_status: Option<&str>,
    ) -> InstallationSyncState {
        InstallationSyncState {
            installation_id: 1,
            webhook_seen_at,
            last_sync_started_at,
            last_sync_completed_at,
            last_sync_status: last_sync_status.map(String::from),
            last_error: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn unknown_installation_needs_sync() {
        let stale_after = ChronoDuration::hours(24);
        let min_interval = ChronoDuration::hours(1);
        assert!(needs_sync(None, stale_after, min_interval));
    }

    #[test]
    fn fresh_webhook_no_sync() {
        let stale_after = ChronoDuration::hours(24);
        let min_interval = ChronoDuration::hours(1);
        let s = state(
            Some(Utc::now() - ChronoDuration::minutes(10)),
            None,
            None,
            None,
        );
        assert!(!needs_sync(Some(&s), stale_after, min_interval));
    }

    #[test]
    fn stale_webhook_needs_sync() {
        let stale_after = ChronoDuration::hours(24);
        let min_interval = ChronoDuration::hours(1);
        let s = state(
            Some(Utc::now() - ChronoDuration::hours(48)),
            None,
            None,
            None,
        );
        assert!(needs_sync(Some(&s), stale_after, min_interval));
    }

    #[test]
    fn recent_sync_throttles_even_if_webhook_stale() {
        let stale_after = ChronoDuration::hours(24);
        let min_interval = ChronoDuration::hours(1);
        let s = state(
            Some(Utc::now() - ChronoDuration::hours(48)),
            Some(Utc::now() - ChronoDuration::minutes(5)),
            Some(Utc::now() - ChronoDuration::minutes(5)),
            Some("success"),
        );
        assert!(!needs_sync(Some(&s), stale_after, min_interval));
    }

    #[test]
    fn failed_sync_retries_once_throttle_clears() {
        let stale_after = ChronoDuration::hours(24);
        let min_interval = ChronoDuration::hours(1);
        // Failed and last_sync was > min_interval ago → retry
        let s = state(
            Some(Utc::now() - ChronoDuration::hours(48)),
            Some(Utc::now() - ChronoDuration::hours(2)),
            Some(Utc::now() - ChronoDuration::hours(2)),
            Some("failed"),
        );
        assert!(needs_sync(Some(&s), stale_after, min_interval));
    }
}
