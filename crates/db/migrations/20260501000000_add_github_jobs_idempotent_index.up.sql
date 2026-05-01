CREATE UNIQUE INDEX idx_github_jobs_board_run_id_type
ON github_jobs (board_run_id, type)
WHERE board_run_id IS NOT NULL;
