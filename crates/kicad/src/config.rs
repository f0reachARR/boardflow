use serde::Deserialize;
use std::path::Path;

use crate::{KicadError, Result};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardflowConfig {
    pub version: u32,
    #[serde(default)]
    pub outputs: Option<OutputsConfig>,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputsConfig {
    pub preset: String,
}

/// Parse a `.boardflow.yml` file into a `BoardflowConfig`.
pub fn parse_boardflow_yml(path: &Path) -> Result<BoardflowConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: BoardflowConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Validate the config follows schema v1 rules.
pub fn validate_schema_v1(config: &BoardflowConfig) -> Result<()> {
    if config.version != 1 {
        return Err(KicadError::ConfigValidation(format!(
            "unsupported version: {}, expected 1",
            config.version
        )));
    }
    if let Some(ref outputs) = config.outputs {
        if outputs.preset != "default" {
            return Err(KicadError::ConfigValidation(format!(
                "unsupported outputs.preset: \"{}\", only \"default\" is allowed",
                outputs.preset
            )));
        }
    }
    Ok(())
}

/// Merge exclude patterns from builtin defaults, user input, and yml config.
/// Returns a deduplicated union of all patterns.
pub fn merge_excludes(builtin: &[&str], input: &[String], yml: &[String]) -> Vec<String> {
    let mut result: Vec<String> = builtin.iter().map(|s| s.to_string()).collect();
    for pattern in input.iter().chain(yml.iter()) {
        if !result.contains(pattern) {
            result.push(pattern.clone());
        }
    }
    result
}
