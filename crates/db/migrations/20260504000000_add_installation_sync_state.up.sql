-- Issue #105: per-installation sync state for webhook freshness / worker reconciliation
CREATE TABLE github_installation_sync_state (
    installation_id BIGINT PRIMARY KEY,
    webhook_seen_at TIMESTAMPTZ,
    last_sync_started_at TIMESTAMPTZ,
    last_sync_completed_at TIMESTAMPTZ,
    last_sync_status TEXT CONSTRAINT github_installation_sync_state_status_check
        CHECK (last_sync_status IN ('success', 'failed')),
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Worker periodically picks installations needing reconciliation. The selector
-- is "webhook_seen_at IS NULL OR webhook_seen_at < cutoff OR last_sync_status = 'failed'",
-- so an index on webhook_seen_at + last_sync_status keeps that scan cheap as
-- installations accumulate.
CREATE INDEX idx_github_installation_sync_state_stale
    ON github_installation_sync_state (webhook_seen_at NULLS FIRST, last_sync_status);

-- Issue #105: list_repositories paginates by (updated_at DESC, github_repository_id DESC).
-- Adding a composite index lets the cursor pagination be index-only.
CREATE INDEX idx_repositories_updated_at_github_id
    ON repositories (updated_at DESC, github_repository_id DESC);
