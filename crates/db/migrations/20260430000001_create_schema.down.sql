-- Reverse of up migration: drop all tables in reverse dependency order
-- First remove circular FK constraints
ALTER TABLE run_checks DROP CONSTRAINT IF EXISTS run_checks_report_artifact_id_fk;
ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS artifacts_source_bundle_id_fk;
ALTER TABLE board_projects DROP CONSTRAINT IF EXISTS board_projects_latest_completed_run_id_fk;

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS board_project_issue_history;
DROP TABLE IF EXISTS github_jobs;
DROP TABLE IF EXISTS boardflow_api_tokens;
DROP TABLE IF EXISTS board_run_diffs;
DROP TABLE IF EXISTS board_run_diff_metadata;
DROP TABLE IF EXISTS board_project_snapshots;
DROP TABLE IF EXISTS run_check_findings;
DROP TABLE IF EXISTS run_checks;
DROP TABLE IF EXISTS artifact_bundles;
DROP TABLE IF EXISTS artifacts;
DROP TABLE IF EXISTS board_runs;
DROP TABLE IF EXISTS board_projects;
DROP TABLE IF EXISTS repositories;
