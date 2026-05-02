CREATE INDEX IF NOT EXISTS idx_board_runs_timeout_sweep
ON board_runs (created_at)
WHERE status IN ('created', 'uploading', 'importing');
