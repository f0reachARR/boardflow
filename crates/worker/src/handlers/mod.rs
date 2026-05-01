pub mod create_dashboard_comment;
pub mod create_issue;
pub mod create_run_result_comment;
pub mod import;
pub mod update_dashboard_comment;

/// Result of a job handler execution.
pub enum HandlerResult {
    /// Job completed successfully.
    Completed,
    /// Job should be rescheduled for retry.
    Reschedule { reason: String, backoff_secs: f64 },
    /// Job failed terminally.
    Failed { reason: String },
}
