use std::env;

#[path = "../src/error.rs"]
mod error;
#[path = "../src/inputs.rs"]
mod inputs;

/// Helper to set env vars for tests and restore them after.
struct EnvGuard {
    keys: Vec<String>,
    originals: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            originals: Vec::new(),
        }
    }

    fn set(&mut self, key: &str, val: &str) {
        self.originals.push((key.to_string(), env::var(key).ok()));
        self.keys.push(key.to_string());
        // SAFETY: tests are run with --test-threads=1 via serial_test or
        // each test uses unique env var keys to avoid races.
        unsafe { env::set_var(key, val) };
    }

    fn remove(&mut self, key: &str) {
        self.originals.push((key.to_string(), env::var(key).ok()));
        self.keys.push(key.to_string());
        unsafe { env::remove_var(key) };
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, original) in &self.originals {
            match original {
                Some(val) => unsafe { env::set_var(key, val) },
                None => unsafe { env::remove_var(key) },
            }
        }
    }
}

#[test]
fn test_parse_inputs_missing_token_fails() {
    let mut g = EnvGuard::new();
    g.remove("INPUT_TOKEN");
    g.remove("INPUT_token");

    let result = inputs::parse_inputs();
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("token"), "error should mention token: {err}");
}

#[test]
fn test_parse_inputs_empty_token_fails() {
    let mut g = EnvGuard::new();
    g.set("INPUT_TOKEN", "");

    let result = inputs::parse_inputs();
    assert!(result.is_err());
}

#[test]
fn test_parse_inputs_valid_defaults() {
    let mut g = EnvGuard::new();
    g.set("INPUT_TOKEN", "ghp_test123");
    g.remove("INPUT_MODE");
    g.remove("INPUT_mode");
    g.remove("INPUT_EXCLUDE_PATHS");
    g.remove("INPUT_EXCLUDE-PATHS");
    g.remove("INPUT_exclude_paths");
    g.remove("INPUT_exclude-paths");
    g.remove("INPUT_API_URL");
    g.remove("INPUT_API-URL");
    g.remove("INPUT_api_url");
    g.remove("INPUT_api-url");
    g.remove("INPUT_FAIL_ON_DRC");
    g.remove("INPUT_FAIL-ON-DRC");
    g.remove("INPUT_fail_on_drc");
    g.remove("INPUT_fail-on-drc");
    g.remove("INPUT_FAIL_ON_ERC");
    g.remove("INPUT_FAIL-ON-ERC");
    g.remove("INPUT_fail_on_erc");
    g.remove("INPUT_fail-on-erc");

    let result = inputs::parse_inputs().unwrap();
    assert_eq!(result.token, "ghp_test123");
    assert_eq!(result.mode, "auto");
    assert_eq!(result.exclude_paths, "");
    assert_eq!(result.api_url, "https://api.boardflow.example.com");
    assert!(!result.fail_on_drc);
    assert!(!result.fail_on_erc);
}

#[test]
fn test_parse_inputs_custom_values() {
    let mut g = EnvGuard::new();
    g.set("INPUT_TOKEN", "token_abc");
    g.set("INPUT_MODE", "force");
    g.set("INPUT_EXCLUDE_PATHS", "foo/**,bar/**");
    g.set("INPUT_API_URL", "https://custom.api.com");
    g.set("INPUT_FAIL_ON_DRC", "true");
    g.set("INPUT_FAIL_ON_ERC", "TRUE");

    let result = inputs::parse_inputs().unwrap();
    assert_eq!(result.token, "token_abc");
    assert_eq!(result.mode, "force");
    assert_eq!(result.exclude_paths, "foo/**,bar/**");
    assert_eq!(result.api_url, "https://custom.api.com");
    assert!(result.fail_on_drc);
    assert!(result.fail_on_erc);
}

#[test]
fn test_parse_inputs_hyphenated_env_vars() {
    let mut g = EnvGuard::new();
    g.set("INPUT_TOKEN", "tok");
    g.remove("INPUT_EXCLUDE_PATHS");
    g.set("INPUT_EXCLUDE-PATHS", "test/**");
    g.remove("INPUT_API_URL");
    g.set("INPUT_API-URL", "https://alt.api.com");
    g.remove("INPUT_FAIL_ON_DRC");
    g.set("INPUT_FAIL-ON-DRC", "true");

    let result = inputs::parse_inputs().unwrap();
    assert_eq!(result.exclude_paths, "test/**");
    assert_eq!(result.api_url, "https://alt.api.com");
    assert!(result.fail_on_drc);
}

#[test]
fn test_parse_github_context_defaults() {
    let mut g = EnvGuard::new();
    g.remove("GITHUB_EVENT_NAME");
    g.remove("GITHUB_REPOSITORY");
    g.remove("GITHUB_SHA");
    g.remove("GITHUB_REF");
    g.remove("GITHUB_REF_NAME");
    g.remove("GITHUB_RUN_ID");
    g.remove("GITHUB_RUN_ATTEMPT");
    g.remove("GITHUB_OUTPUT");
    g.remove("GITHUB_STEP_SUMMARY");
    g.remove("GITHUB_WORKSPACE");

    let ctx = inputs::parse_github_context().unwrap();
    assert_eq!(ctx.event_name, "");
    assert_eq!(ctx.run_attempt, "1");
    assert_eq!(ctx.workspace.to_str().unwrap(), "/github/workspace");
}

#[test]
fn test_parse_github_context_splits_repository() {
    let mut g = EnvGuard::new();
    g.set("GITHUB_REPOSITORY", "myorg/myrepo");
    g.set("GITHUB_EVENT_NAME", "push");
    g.set("GITHUB_SHA", "abc123");
    g.set("GITHUB_REF", "refs/heads/main");
    g.set("GITHUB_REF_NAME", "main");
    g.set("GITHUB_RUN_ID", "12345");
    g.set("GITHUB_RUN_ATTEMPT", "2");
    g.set("GITHUB_OUTPUT", "/tmp/output");
    g.set("GITHUB_STEP_SUMMARY", "/tmp/summary");
    g.set("GITHUB_WORKSPACE", "/work");

    let ctx = inputs::parse_github_context().unwrap();
    assert_eq!(ctx.owner, "myorg");
    assert_eq!(ctx.repo_name, "myrepo");
    assert_eq!(ctx.event_name, "push");
    assert_eq!(ctx.sha, "abc123");
    assert_eq!(ctx.git_ref, "refs/heads/main");
    assert_eq!(ctx.ref_name, "main");
    assert_eq!(ctx.run_id, "12345");
    assert_eq!(ctx.run_attempt, "2");
    assert_eq!(ctx.workspace.to_str().unwrap(), "/work");
}

#[test]
fn test_parse_github_context_no_slash_in_repo() {
    let mut g = EnvGuard::new();
    g.set("GITHUB_REPOSITORY", "noslash");

    let ctx = inputs::parse_github_context().unwrap();
    assert_eq!(ctx.owner, "noslash");
    assert_eq!(ctx.repo_name, "");
}
