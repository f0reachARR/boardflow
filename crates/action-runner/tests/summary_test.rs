use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[path = "../src/error.rs"]
mod error;
#[path = "../src/summary.rs"]
mod summary;

use summary::ProjectResult;

#[test]
fn test_set_output_appends_key_value() {
    let dir = TempDir::new().unwrap();
    let output_file = dir.path().join("output");

    summary::set_output("key1", "value1", &output_file).unwrap();
    summary::set_output("key2", "value2", &output_file).unwrap();

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("key1=value1\n"));
    assert!(content.contains("key2=value2\n"));
}

#[test]
fn test_set_output_empty_path_noop() {
    let empty_path = PathBuf::from("");
    // Should not error
    summary::set_output("key", "val", &empty_path).unwrap();
}

#[test]
fn test_write_job_summary_creates_markdown_table() {
    let dir = TempDir::new().unwrap();
    let summary_file = dir.path().join("summary.md");

    let results = vec![
        ProjectResult {
            path: "board/board.kicad_pro".to_string(),
            status: "success".to_string(),
            error: None,
        },
        ProjectResult {
            path: "other/hw.kicad_pro".to_string(),
            status: "error".to_string(),
            error: Some("upload failed".to_string()),
        },
        ProjectResult {
            path: "skip/x.kicad_pro".to_string(),
            status: "skipped".to_string(),
            error: None,
        },
    ];

    summary::write_job_summary(&results, &summary_file).unwrap();

    let content = fs::read_to_string(&summary_file).unwrap();
    assert!(content.contains("## BoardFlow Results"));
    assert!(content.contains("| Project | Status | Error |"));
    assert!(content.contains("board/board.kicad_pro"));
    assert!(content.contains("success"));
    assert!(content.contains("upload failed"));
    assert!(content.contains("skipped"));
}

#[test]
fn test_write_job_summary_empty_results() {
    let dir = TempDir::new().unwrap();
    let summary_file = dir.path().join("summary.md");

    summary::write_job_summary(&[], &summary_file).unwrap();

    let content = fs::read_to_string(&summary_file).unwrap();
    assert!(content.contains("## BoardFlow Results"));
    assert!(content.contains("| Project | Status | Error |"));
}

#[test]
fn test_write_unsupported_event_summary() {
    let dir = TempDir::new().unwrap();
    let summary_file = dir.path().join("summary.md");

    summary::write_unsupported_event_summary("pull_request", &summary_file).unwrap();

    let content = fs::read_to_string(&summary_file).unwrap();
    assert!(content.contains("pull_request"));
    assert!(content.contains("Skipped"));
    assert!(content.contains("not supported"));
}

#[test]
fn test_write_unsupported_event_summary_empty_path() {
    let empty_path = PathBuf::from("");
    summary::write_unsupported_event_summary("pull_request", &empty_path).unwrap();
}
