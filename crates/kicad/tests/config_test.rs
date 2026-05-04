use std::fs;

use boardflow_kicad::config::{merge_excludes, parse_boardflow_yml, validate_schema_v1};
use tempfile::TempDir;

#[test]
fn parse_valid_boardflow_yml() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        r#"
version: 1
outputs:
  preset: default
exclude_paths:
  - "docs/**"
  - "test/**"
"#,
    )
    .unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    assert_eq!(config.version, 1);
    assert_eq!(
        config.outputs.as_ref().unwrap().preset.as_deref(),
        Some("default")
    );
    assert_eq!(config.exclude_paths.len(), 2);
    assert_eq!(config.exclude_paths[0], "docs/**");
    assert_eq!(config.exclude_paths[1], "test/**");
}

#[test]
fn parse_minimal_boardflow_yml() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "version: 1\n").unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    assert_eq!(config.version, 1);
    assert!(config.outputs.is_none());
    assert!(config.exclude_paths.is_empty());
}

#[test]
fn parse_boardflow_yml_rejects_unknown_fields() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        r#"
version: 1
unknown_field: hello
outputs:
  preset: default
"#,
    )
    .unwrap();

    let result = parse_boardflow_yml(&yml_path);
    assert!(result.is_err(), "unknown fields should be rejected");
}

#[test]
fn parse_boardflow_yml_rejects_unknown_outputs_field() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        r#"
version: 1
outputs:
  preset: default
  extra: true
"#,
    )
    .unwrap();

    let result = parse_boardflow_yml(&yml_path);
    assert!(
        result.is_err(),
        "unknown fields in outputs should be rejected"
    );
}

#[test]
fn validate_schema_v1_accepts_version_1() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "version: 1\n").unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    assert!(validate_schema_v1(&config).is_ok());
}

#[test]
fn validate_schema_v1_rejects_version_0() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "version: 0\n").unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    let result = validate_schema_v1(&config);
    assert!(result.is_err());
}

#[test]
fn validate_schema_v1_rejects_version_2() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "version: 2\n").unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    let result = validate_schema_v1(&config);
    assert!(result.is_err());
}

#[test]
fn validate_schema_v1_accepts_preset_default() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        "version: 1\noutputs:\n  preset: default\n",
    )
    .unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    assert!(validate_schema_v1(&config).is_ok());
}

#[test]
fn validate_schema_v1_accepts_no_preset() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "version: 1\noutputs:\n  preset:\n").unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    assert!(validate_schema_v1(&config).is_ok());
}

#[test]
fn validate_schema_v1_rejects_preset_custom() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        "version: 1\noutputs:\n  preset: custom\n",
    )
    .unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    let result = validate_schema_v1(&config);
    assert!(result.is_err(), "preset 'custom' should be rejected");
}

#[test]
fn validate_schema_v1_rejects_preset_full() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(
        &yml_path,
        "version: 1\noutputs:\n  preset: full\n",
    )
    .unwrap();

    let config = parse_boardflow_yml(&yml_path).unwrap();
    let result = validate_schema_v1(&config);
    assert!(result.is_err(), "preset 'full' should be rejected");
}

#[test]
fn parse_invalid_yaml_returns_error() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join(".boardflow.yml");
    fs::write(&yml_path, "not: [valid: yaml: content").unwrap();

    let result = parse_boardflow_yml(&yml_path);
    assert!(result.is_err());
}

#[test]
fn parse_nonexistent_file_returns_error() {
    let tmp = TempDir::new().unwrap();
    let yml_path = tmp.path().join("nonexistent.yml");

    let result = parse_boardflow_yml(&yml_path);
    assert!(result.is_err());
}

#[test]
fn merge_excludes_combines_all_sources() {
    let builtin = &["**/*.lck", "**/*.bak"];
    let input = vec!["custom/**".to_string()];
    let yml = vec!["docs/**".to_string()];

    let merged = merge_excludes(builtin, &input, &yml);
    assert_eq!(merged.len(), 4);
    assert!(merged.contains(&"**/*.lck".to_string()));
    assert!(merged.contains(&"**/*.bak".to_string()));
    assert!(merged.contains(&"custom/**".to_string()));
    assert!(merged.contains(&"docs/**".to_string()));
}

#[test]
fn merge_excludes_deduplicates() {
    let builtin = &["**/*.lck"];
    let input = vec!["**/*.lck".to_string()]; // duplicate
    let yml = vec!["**/*.lck".to_string()]; // duplicate

    let merged = merge_excludes(builtin, &input, &yml);
    assert_eq!(merged.len(), 1);
}

#[test]
fn merge_excludes_empty_inputs() {
    let builtin: &[&str] = &[];
    let input: Vec<String> = vec![];
    let yml: Vec<String> = vec![];

    let merged = merge_excludes(builtin, &input, &yml);
    assert!(merged.is_empty());
}
