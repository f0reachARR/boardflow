CREATE INDEX idx_github_jobs_dequeue
ON github_jobs (run_after, created_at)
WHERE status = 'pending';
