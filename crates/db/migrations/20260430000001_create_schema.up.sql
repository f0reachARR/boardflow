-- Issue #2: Create all application tables

-- 1. repositories (no FK dependencies)
CREATE TABLE repositories (
    id UUID PRIMARY KEY,
    github_repository_id BIGINT NOT NULL UNIQUE,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    installation_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- 2. board_projects (→ repositories)
CREATE TABLE board_projects (
    id UUID PRIMARY KEY,
    repository_id UUID NOT NULL REFERENCES repositories(id),
    project_path TEXT NOT NULL,
    project_dir TEXT NOT NULL,
    display_name TEXT NOT NULL,
    issue_number INTEGER,
    issue_node_id TEXT,
    issue_url TEXT,
    issue_sync_status TEXT NOT NULL DEFAULT 'pending' CONSTRAINT board_projects_issue_sync_status_check CHECK (issue_sync_status IN ('pending', 'syncing', 'synced', 'failed')),
    dashboard_comment_id BIGINT,
    recreate_issue_on_update BOOLEAN NOT NULL DEFAULT true,
    latest_tree_hash TEXT,
    latest_completed_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (repository_id, project_path)
);

-- 3. board_runs (→ board_projects)
CREATE TABLE board_runs (
    id UUID PRIMARY KEY,
    board_project_id UUID NOT NULL REFERENCES board_projects(id),
    commit_sha TEXT NOT NULL,
    branch TEXT NOT NULL,
    ref TEXT NOT NULL,
    github_run_id BIGINT NOT NULL,
    github_run_attempt INTEGER NOT NULL,
    tree_hash TEXT,
    status TEXT NOT NULL DEFAULT 'created' CONSTRAINT board_runs_status_check CHECK (status IN ('created', 'uploading', 'importing', 'completed', 'failed', 'timed_out')),
    erc_status TEXT CONSTRAINT board_runs_erc_status_check CHECK (erc_status IN ('passed', 'failed', 'skipped')),
    erc_errors INTEGER NOT NULL DEFAULT 0,
    erc_warnings INTEGER NOT NULL DEFAULT 0,
    drc_status TEXT CONSTRAINT board_runs_drc_status_check CHECK (drc_status IN ('passed', 'failed', 'skipped')),
    drc_errors INTEGER NOT NULL DEFAULT 0,
    drc_warnings INTEGER NOT NULL DEFAULT 0,
    review_status TEXT NOT NULL DEFAULT 'pending' CONSTRAINT board_runs_review_status_check CHECK (review_status IN ('pending', 'ready', 'no_baseline', 'failed')),
    diff_status TEXT NOT NULL DEFAULT 'pending' CONSTRAINT board_runs_diff_status_check CHECK (diff_status IN ('pending', 'ready', 'no_baseline', 'unavailable', 'failed')),
    expires_at TIMESTAMPTZ,
    timed_out_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    UNIQUE (board_project_id, github_run_id, github_run_attempt),
    CONSTRAINT board_runs_id_board_project_id_unique UNIQUE (id, board_project_id)
);

-- 4. artifact_bundles (→ board_runs)
CREATE TABLE artifact_bundles (
    id UUID PRIMARY KEY,
    board_run_id UUID NOT NULL REFERENCES board_runs(id),
    intake_mode TEXT NOT NULL,
    staging_object_key TEXT,
    original_filename TEXT,
    sha256 TEXT,
    size_bytes BIGINT,
    status TEXT NOT NULL DEFAULT 'pending' CONSTRAINT artifact_bundles_status_check CHECK (status IN ('pending', 'validating', 'importing', 'completed', 'failed')),
    error_message TEXT,
    received_at TIMESTAMPTZ NOT NULL,
    validated_at TIMESTAMPTZ,
    delete_after TIMESTAMPTZ
);

-- 5. artifacts (→ board_runs)
CREATE TABLE artifacts (
    id UUID PRIMARY KEY,
    board_run_id UUID NOT NULL REFERENCES board_runs(id),
    type TEXT NOT NULL,
    status TEXT NOT NULL CONSTRAINT artifacts_status_check CHECK (status IN ('available', 'missing', 'failed', 'skipped')),
    filename TEXT,
    source_path TEXT,
    logical_name TEXT,
    content_type TEXT,
    storage_key TEXT,
    sha256 TEXT,
    size_bytes BIGINT,
    status_reason TEXT,
    error_message TEXT,
    source_bundle_id UUID,
    created_at TIMESTAMPTZ NOT NULL
);

-- 6. run_checks (→ board_runs)
CREATE TABLE run_checks (
    id UUID PRIMARY KEY,
    board_run_id UUID NOT NULL REFERENCES board_runs(id),
    check_kind TEXT NOT NULL CONSTRAINT run_checks_check_kind_check CHECK (check_kind IN ('erc', 'drc')),
    tool_name TEXT,
    tool_version TEXT,
    status TEXT NOT NULL CONSTRAINT run_checks_status_check CHECK (status IN ('passed', 'failed', 'skipped')),
    error_count INTEGER NOT NULL DEFAULT 0,
    warning_count INTEGER NOT NULL DEFAULT 0,
    notice_count INTEGER NOT NULL DEFAULT 0,
    report_artifact_id UUID,
    raw_summary_json JSONB,
    created_at TIMESTAMPTZ NOT NULL
);

-- 7. run_check_findings (→ run_checks)
CREATE TABLE run_check_findings (
    id UUID PRIMARY KEY,
    run_check_id UUID NOT NULL REFERENCES run_checks(id),
    severity TEXT NOT NULL CONSTRAINT run_check_findings_severity_check CHECK (severity IN ('error', 'warning', 'notice')),
    rule_code TEXT,
    title TEXT,
    message TEXT,
    subject_kind TEXT CONSTRAINT run_check_findings_subject_kind_check CHECK (subject_kind IN ('schematic', 'pcb', 'net', 'footprint', 'symbol')),
    subject_ref TEXT,
    sheet_path TEXT,
    pcb_layer TEXT,
    x_um INTEGER,
    y_um INTEGER,
    bbox_json JSONB,
    raw_payload_json JSONB,
    sort_index INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL
);

-- 8. board_project_snapshots (→ board_projects, board_runs)
CREATE TABLE board_project_snapshots (
    id UUID PRIMARY KEY,
    board_project_id UUID NOT NULL REFERENCES board_projects(id),
    board_run_id UUID NOT NULL REFERENCES board_runs(id),
    tree_hash TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    file_hashes_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

-- 9. board_run_diff_metadata (→ board_runs)
CREATE TABLE board_run_diff_metadata (
    id UUID PRIMARY KEY,
    board_run_id UUID NOT NULL REFERENCES board_runs(id) UNIQUE,
    file_hashes_json JSONB,
    bom_summary_json JSONB,
    checks_summary_json JSONB,
    artifacts_summary_json JSONB,
    previews_json JSONB,
    created_at TIMESTAMPTZ NOT NULL
);

-- 10. board_run_diffs (→ board_runs)
CREATE TABLE board_run_diffs (
    id UUID PRIMARY KEY,
    board_run_id UUID NOT NULL REFERENCES board_runs(id) UNIQUE,
    base_board_run_id UUID REFERENCES board_runs(id),
    status TEXT NOT NULL CONSTRAINT board_run_diffs_status_check CHECK (status IN ('ready', 'no_baseline', 'unavailable', 'failed')),
    summary_json JSONB,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

-- 11. boardflow_api_tokens (→ repositories)
CREATE TABLE boardflow_api_tokens (
    id UUID PRIMARY KEY,
    installation_id BIGINT NOT NULL,
    repository_id UUID NOT NULL REFERENCES repositories(id),
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

-- 12. github_jobs (→ repositories, board_projects, board_runs)
CREATE TABLE github_jobs (
    id UUID PRIMARY KEY,
    installation_id BIGINT NOT NULL,
    repository_id UUID NOT NULL REFERENCES repositories(id),
    board_project_id UUID REFERENCES board_projects(id),
    board_run_id UUID REFERENCES board_runs(id),
    type TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CONSTRAINT github_jobs_status_check CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    run_after TIMESTAMPTZ NOT NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- 13. board_project_issue_history (→ board_projects)
CREATE TABLE board_project_issue_history (
    id UUID PRIMARY KEY,
    board_project_id UUID NOT NULL REFERENCES board_projects(id),
    issue_number INTEGER NOT NULL,
    issue_node_id TEXT NOT NULL,
    issue_url TEXT NOT NULL,
    reason TEXT NOT NULL CONSTRAINT board_project_issue_history_reason_check CHECK (reason IN ('recreated', 'deleted', 'manual_archive')),
    replaced_by_issue_node_id TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

-- 14. Circular FK constraints (ALTER TABLE)
ALTER TABLE board_projects ADD CONSTRAINT board_projects_latest_completed_run_id_fk FOREIGN KEY (latest_completed_run_id, id) REFERENCES board_runs(id, board_project_id);
ALTER TABLE artifacts ADD CONSTRAINT artifacts_source_bundle_id_fk FOREIGN KEY (source_bundle_id) REFERENCES artifact_bundles(id);
ALTER TABLE run_checks ADD CONSTRAINT run_checks_report_artifact_id_fk FOREIGN KEY (report_artifact_id) REFERENCES artifacts(id);

-- 15. Indexes on FK columns
CREATE INDEX idx_board_projects_repository_id ON board_projects(repository_id);
CREATE INDEX idx_board_projects_latest_completed_run_id ON board_projects(latest_completed_run_id);
CREATE INDEX idx_board_runs_board_project_id ON board_runs(board_project_id);
CREATE INDEX idx_artifact_bundles_board_run_id ON artifact_bundles(board_run_id);
CREATE INDEX idx_artifacts_board_run_id ON artifacts(board_run_id);
CREATE INDEX idx_artifacts_source_bundle_id ON artifacts(source_bundle_id);
CREATE INDEX idx_run_checks_board_run_id ON run_checks(board_run_id);
CREATE INDEX idx_run_checks_report_artifact_id ON run_checks(report_artifact_id);
CREATE INDEX idx_run_check_findings_run_check_id ON run_check_findings(run_check_id);
CREATE INDEX idx_board_project_snapshots_board_project_id ON board_project_snapshots(board_project_id);
CREATE INDEX idx_board_project_snapshots_board_run_id ON board_project_snapshots(board_run_id);
CREATE INDEX idx_board_run_diffs_base_board_run_id ON board_run_diffs(base_board_run_id);
CREATE INDEX idx_boardflow_api_tokens_repository_id ON boardflow_api_tokens(repository_id);
CREATE INDEX idx_boardflow_api_tokens_installation_id ON boardflow_api_tokens(installation_id);
CREATE INDEX idx_github_jobs_repository_id ON github_jobs(repository_id);
CREATE INDEX idx_github_jobs_board_project_id ON github_jobs(board_project_id);
CREATE INDEX idx_github_jobs_board_run_id ON github_jobs(board_run_id);
CREATE INDEX idx_board_project_issue_history_board_project_id ON board_project_issue_history(board_project_id);

-- 16. Partial index for pending jobs
CREATE INDEX idx_github_jobs_pending ON github_jobs(run_after) WHERE status = 'pending';

-- 17. Unique index for artifact deduplication within a run (NULLS NOT DISTINCT for nullable source_path)
CREATE UNIQUE INDEX idx_artifacts_run_type_path ON artifacts(board_run_id, type, source_path) NULLS NOT DISTINCT;
