use boardflow_domain::models::board_run::BoardRun;
use uuid::Uuid;

/// Generate the issue body for a board project issue.
pub fn issue_body(
    github_repository_id: i64,
    project_path: &str,
    board_project_id: Uuid,
    base_url: &str,
) -> String {
    format!(
        r#"<!-- boardflow:repository_id={github_repository_id} -->
<!-- boardflow:project_path={project_path} -->

# Board Project

KiCad project:

`{project_path}`

This issue tracks design, fabrication, assembly, and verification for this board.

## BoardFlow

Latest board page:

{base_url}/repositories/{github_repository_id}/boards/{board_project_id}"#
    )
}

/// Generate the issue title for a board project.
pub fn issue_title(display_name: &str) -> String {
    format!("[Board] {display_name}")
}

/// Generate the dashboard comment body.
pub fn dashboard_comment(
    project_path: &str,
    board_run: &BoardRun,
    board_project_id: Uuid,
    github_repository_id: i64,
    base_url: &str,
) -> String {
    let commit_sha_short = &board_run.commit_sha[..7.min(board_run.commit_sha.len())];
    let branch = &board_run.branch;

    let erc_result = check_result_text(board_run.erc_status, board_run.erc_errors, board_run.erc_warnings);
    let drc_result = check_result_text(board_run.drc_status, board_run.drc_errors, board_run.drc_warnings);

    let board_page_url = format!(
        "{base_url}/repositories/{github_repository_id}/boards/{board_project_id}"
    );
    let diff_url = format!(
        "{base_url}/repositories/{github_repository_id}/boards/{board_project_id}/runs/{}/diff",
        board_run.id
    );

    format!(
        r#"<!-- boardflow:comment_type=dashboard -->
<!-- boardflow:project_path={project_path} -->

## BoardFlow Dashboard

Latest run: `{commit_sha_short}` on `{branch}`

| Item | Link |
|---|---|
| Board page | {board_page_url} |
| Latest diff | {diff_url} |

### Latest status

| Check | Result |
|---|---|
| ERC | {erc_result} |
| DRC | {drc_result} |

Last updated by BoardFlow."#
    )
}

/// Generate the run result comment body.
pub fn run_result_comment(
    board_run: &BoardRun,
    board_project_id: Uuid,
    github_repository_id: i64,
    base_url: &str,
) -> String {
    let commit_sha_short = &board_run.commit_sha[..7.min(board_run.commit_sha.len())];

    let erc_result = check_result_text(board_run.erc_status, board_run.erc_errors, board_run.erc_warnings);
    let drc_result = check_result_text(board_run.drc_status, board_run.drc_errors, board_run.drc_warnings);

    let run_url = format!(
        "{base_url}/repositories/{github_repository_id}/boards/{board_project_id}/runs/{}",
        board_run.id
    );
    let diff_url = format!(
        "{base_url}/repositories/{github_repository_id}/boards/{board_project_id}/runs/{}/diff",
        board_run.id
    );

    format!(
        r#"<!-- boardflow:comment_type=run_result -->
<!-- boardflow:board_run_id={} -->

## BoardFlow Run Result

Commit: `{commit_sha_short}`
Run: {run_url}
Diff: {diff_url}

| Check | Result |
|---|---|
| ERC | {erc_result} |
| DRC | {drc_result} |"#,
        board_run.id
    )
}

/// Determine whether a run result comment should be posted.
/// Returns true if:
/// - New DRC/ERC errors appeared
/// - Previous run passed → current run failed
/// - Previous run failed → current run passed
pub fn should_post_run_result(current: &BoardRun, previous: Option<&BoardRun>) -> bool {
    use boardflow_domain::models::board_run::CheckStatus;

    // Always post if there's no previous run (first completed run)
    let Some(prev) = previous else {
        return true;
    };

    // Check ERC status transition
    let erc_changed = match (prev.erc_status, current.erc_status) {
        (Some(CheckStatus::Passed), Some(CheckStatus::Failed)) => true,
        (Some(CheckStatus::Failed), Some(CheckStatus::Passed)) => true,
        _ => false,
    };

    // Check DRC status transition
    let drc_changed = match (prev.drc_status, current.drc_status) {
        (Some(CheckStatus::Passed), Some(CheckStatus::Failed)) => true,
        (Some(CheckStatus::Failed), Some(CheckStatus::Passed)) => true,
        _ => false,
    };

    // Check if new errors appeared
    let new_erc_errors = current.erc_errors > prev.erc_errors;
    let new_drc_errors = current.drc_errors > prev.drc_errors;

    erc_changed || drc_changed || new_erc_errors || new_drc_errors
}

fn check_result_text(
    status: Option<boardflow_domain::models::board_run::CheckStatus>,
    errors: i32,
    warnings: i32,
) -> String {
    use boardflow_domain::models::board_run::CheckStatus;

    match status {
        Some(CheckStatus::Passed) => "✅ Passed".to_string(),
        Some(CheckStatus::Failed) => {
            format!("❌ Failed ({errors} errors, {warnings} warnings)")
        }
        Some(CheckStatus::Skipped) => "⏭️ Skipped".to_string(),
        None => "—".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boardflow_domain::models::board_run::{BoardRun, BoardRunStatus, CheckStatus, DiffStatus, ReviewStatus};
    use chrono::Utc;

    fn make_run(
        erc_status: Option<CheckStatus>,
        erc_errors: i32,
        drc_status: Option<CheckStatus>,
        drc_errors: i32,
    ) -> BoardRun {
        BoardRun {
            id: Uuid::now_v7(),
            board_project_id: Uuid::now_v7(),
            commit_sha: "abc1234567890".to_string(),
            branch: "main".to_string(),
            r#ref: "refs/heads/main".to_string(),
            github_run_id: 1,
            github_run_attempt: 1,
            tree_hash: Some("treehash".to_string()),
            status: BoardRunStatus::Completed,
            erc_status,
            erc_errors,
            erc_warnings: 0,
            drc_status,
            drc_errors,
            drc_warnings: 0,
            review_status: ReviewStatus::Ready,
            diff_status: DiffStatus::Ready,
            expires_at: None,
            timed_out_at: None,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
        }
    }

    #[test]
    fn test_issue_title() {
        assert_eq!(issue_title("LightStick"), "[Board] LightStick");
    }

    #[test]
    fn test_issue_body_contains_markers() {
        let body = issue_body(12345, "hardware/LightStick.kicad_pro", Uuid::nil(), "https://boardflow.example.com");
        assert!(body.contains("<!-- boardflow:repository_id=12345 -->"));
        assert!(body.contains("<!-- boardflow:project_path=hardware/LightStick.kicad_pro -->"));
        assert!(body.contains("`hardware/LightStick.kicad_pro`"));
        assert!(body.contains("https://boardflow.example.com/repositories/12345/boards/"));
    }

    #[test]
    fn test_dashboard_comment_contains_markers() {
        let run = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Failed), 2);
        let body = dashboard_comment(
            "hw/board.kicad_pro",
            &run,
            run.board_project_id,
            99,
            "https://bf.dev",
        );
        assert!(body.contains("<!-- boardflow:comment_type=dashboard -->"));
        assert!(body.contains("<!-- boardflow:project_path=hw/board.kicad_pro -->"));
        assert!(body.contains("abc1234"));
        assert!(body.contains("✅ Passed"));
        assert!(body.contains("❌ Failed (2 errors, 0 warnings)"));
        assert!(body.contains("Last updated by BoardFlow."));
    }

    #[test]
    fn test_run_result_comment_contains_markers() {
        let run = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Skipped), 0);
        let body = run_result_comment(&run, run.board_project_id, 42, "https://bf.dev");
        assert!(body.contains("<!-- boardflow:comment_type=run_result -->"));
        assert!(body.contains(&format!("<!-- boardflow:board_run_id={} -->", run.id)));
        assert!(body.contains("✅ Passed"));
        assert!(body.contains("⏭️ Skipped"));
    }

    #[test]
    fn test_should_post_run_result_first_run() {
        let current = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Passed), 0);
        assert!(should_post_run_result(&current, None));
    }

    #[test]
    fn test_should_post_run_result_pass_to_fail() {
        let prev = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Passed), 0);
        let current = make_run(Some(CheckStatus::Failed), 1, Some(CheckStatus::Passed), 0);
        assert!(should_post_run_result(&current, Some(&prev)));
    }

    #[test]
    fn test_should_post_run_result_fail_to_pass() {
        let prev = make_run(Some(CheckStatus::Failed), 1, Some(CheckStatus::Passed), 0);
        let current = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Passed), 0);
        assert!(should_post_run_result(&current, Some(&prev)));
    }

    #[test]
    fn test_should_post_run_result_new_errors() {
        let prev = make_run(Some(CheckStatus::Failed), 1, Some(CheckStatus::Failed), 2);
        let current = make_run(Some(CheckStatus::Failed), 1, Some(CheckStatus::Failed), 3);
        assert!(should_post_run_result(&current, Some(&prev)));
    }

    #[test]
    fn test_should_not_post_run_result_no_change() {
        let prev = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Passed), 0);
        let current = make_run(Some(CheckStatus::Passed), 0, Some(CheckStatus::Passed), 0);
        assert!(!should_post_run_result(&current, Some(&prev)));
    }

    #[test]
    fn test_should_not_post_run_result_same_failure() {
        let prev = make_run(Some(CheckStatus::Failed), 2, Some(CheckStatus::Failed), 3);
        let current = make_run(Some(CheckStatus::Failed), 2, Some(CheckStatus::Failed), 3);
        assert!(!should_post_run_result(&current, Some(&prev)));
    }

    #[test]
    fn test_should_not_post_run_result_fewer_errors() {
        let prev = make_run(Some(CheckStatus::Failed), 3, Some(CheckStatus::Failed), 5);
        let current = make_run(Some(CheckStatus::Failed), 2, Some(CheckStatus::Failed), 4);
        assert!(!should_post_run_result(&current, Some(&prev)));
    }
}
