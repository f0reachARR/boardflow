use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

use crate::Result;

pub const BUILTIN_EXCLUDES: &[&str] = &[
    "**/*.lck",
    "**/*.bak",
    "**/*-backups/**",
    "**/fp-info-cache",
    "**/.DS_Store",
    "**/output/**",
    "**/outputs/**",
    "**/fabrication/**",
    "**/gerber/**",
    "**/gerbers/**",
];

/// Check if a relative path matches any of the given glob patterns.
pub fn is_excluded(path: &str, patterns: &[String]) -> bool {
    let globset = build_globset(patterns);
    globset.is_match(path)
}

/// List all files in `dir` recursively, excluding those matching `excludes` patterns.
pub fn list_project_files(dir: &Path, excludes: &[String]) -> Result<Vec<PathBuf>> {
    let globset = build_globset(excludes);
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel_path = entry
            .path()
            .strip_prefix(dir)
            .unwrap_or(entry.path());
        let rel_str = rel_path.to_str().unwrap_or_default();
        if !globset.is_match(rel_str) {
            files.push(entry.into_path());
        }
    }

    Ok(files)
}

/// Compute the SHA256 hash of a single file, returning the hex string.
pub fn compute_file_sha256(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Compute a tree hash over all non-excluded files in a directory.
///
/// Algorithm:
/// 1. List all files (excluding patterns)
/// 2. Sort by relative path (UTF-8 byte order)
/// 3. For each file: `"{rel_path}\0{sha256_hex}\n"`
/// 4. SHA256 the concatenation
pub fn compute_tree_hash(dir: &Path, excludes: &[String]) -> Result<String> {
    let files = list_project_files(dir, excludes)?;

    // Build sorted (relative_path, absolute_path) pairs
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|abs_path| {
            let rel = abs_path.strip_prefix(dir).ok()?;
            Some((rel.to_str()?.to_string(), abs_path))
        })
        .collect();

    // Sort by relative path (byte order)
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    // Build the content to hash
    let mut content = String::new();
    for (rel_path, abs_path) in &entries {
        let file_hash = compute_file_sha256(abs_path)?;
        content.push_str(rel_path);
        content.push('\0');
        content.push_str(&file_hash);
        content.push('\n');
    }

    // Hash the entire content
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

fn build_globset(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap())
}
