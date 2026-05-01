-- Issue #36: Add indexes for findings keyset pagination and run_checks uniqueness

-- Composite index for keyset pagination on run_check_findings
CREATE INDEX idx_run_check_findings_keyset
ON run_check_findings (run_check_id, sort_index, id);

-- Composite index for keyset pagination with severity filter
CREATE INDEX idx_run_check_findings_severity_keyset
ON run_check_findings (run_check_id, severity, sort_index, id);

-- Unique constraint on run_checks(board_run_id, check_kind) to guarantee
-- find_by_board_run_and_kind returns at most one row
CREATE UNIQUE INDEX idx_run_checks_board_run_kind
ON run_checks (board_run_id, check_kind);
