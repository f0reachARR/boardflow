//! Job processing utilities for the BoardFlow worker.
//!
//! SQL operations live in `boardflow_db::queries::github_job`.
//! This crate provides higher-level job processing constants and helpers.

pub const MAX_ATTEMPTS: i32 = 5;
pub const BASE_BACKOFF_SECS: f64 = 10.0;

/// Calculate exponential backoff for retry
pub fn backoff_secs(attempts: i32) -> f64 {
    BASE_BACKOFF_SECS * 3_f64.powi(attempts)
}
