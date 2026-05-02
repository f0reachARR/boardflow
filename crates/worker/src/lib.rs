//! BoardFlow worker library.
//!
//! This module exposes worker internals for integration testing.

pub mod comment_body;
pub mod config;
pub mod dispatcher;
pub mod handlers;

pub use config::WorkerConfig;
