use std::fs;

use boardflow_kicad::hash::{
    BUILTIN_EXCLUDES, compute_file_sha256, compute_tree_hash, is_excluded, list_project_files,
};
use tempfile::TempDir;

#[test]
fn is_excluded_matches_lck_pattern() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded("project.kicad_pro.lck", &patterns));
    assert!(is_excluded("sub/dir/file.lck", &patterns));
}

#[test]
fn is_excluded_matches_bak_pattern() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded("file.bak", &patterns));
    assert!(is_excluded("deep/nested/file.bak", &patterns));
}

#[test]
fn is_excluded_matches_backups_dir() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded("proj-backups/file.kicad_sch", &patterns));
    assert!(is_excluded("sub/proj-backups/file.txt", &patterns));
}

#[test]
fn is_excluded_matches_fp_info_cache() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded("fp-info-cache", &patterns));
    assert!(is_excluded("sub/fp-info-cache", &patterns));
}

#[test]
fn is_excluded_matches_ds_store() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded(".DS_Store", &patterns));
    assert!(is_excluded("sub/.DS_Store", &patterns));
}

#[test]
fn is_excluded_matches_output_dirs() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(is_excluded("output/gerber.gbr", &patterns));
    assert!(is_excluded("outputs/bom.csv", &patterns));
    assert!(is_excluded("fabrication/drill.drl", &patterns));
    assert!(is_excluded("gerber/file.gbr", &patterns));
    assert!(is_excluded("gerbers/file.gbr", &patterns));
}

#[test]
fn is_excluded_does_not_match_normal_files() {
    let patterns: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    assert!(!is_excluded("Board.kicad_pro", &patterns));
    assert!(!is_excluded("Board.kicad_pcb", &patterns));
    assert!(!is_excluded("Board.kicad_sch", &patterns));
    assert!(!is_excluded("sub/component.kicad_sym", &patterns));
}

#[test]
fn is_excluded_custom_pattern() {
    let patterns = vec!["docs/**".to_string()];
    assert!(is_excluded("docs/readme.md", &patterns));
    assert!(!is_excluded("src/main.rs", &patterns));
}

#[test]
fn list_project_files_excludes_patterns() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("Board.kicad_pro"), "{}").unwrap();
    fs::write(dir.join("Board.kicad_pcb"), "").unwrap();
    fs::write(dir.join("Board.kicad_pro.lck"), "").unwrap();
    fs::write(dir.join("file.bak"), "").unwrap();
    fs::write(dir.join("fp-info-cache"), "").unwrap();

    let excludes: Vec<String> = BUILTIN_EXCLUDES.iter().map(|s| s.to_string()).collect();
    let files = list_project_files(dir, &excludes).unwrap();

    let names: Vec<&str> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert!(names.contains(&"Board.kicad_pro"));
    assert!(names.contains(&"Board.kicad_pcb"));
    assert!(!names.contains(&"Board.kicad_pro.lck"));
    assert!(!names.contains(&"file.bak"));
    assert!(!names.contains(&"fp-info-cache"));
}

#[test]
fn list_project_files_recurses_directories() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("top.txt"), "top").unwrap();
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("nested.txt"), "nested").unwrap();

    let files = list_project_files(dir, &[]).unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn compute_file_sha256_known_value() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    fs::write(&file_path, "hello\n").unwrap();

    let hash = compute_file_sha256(&file_path).unwrap();
    // SHA256 of "hello\n"
    assert_eq!(
        hash,
        "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
    );
}

#[test]
fn compute_file_sha256_empty_file() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("empty.txt");
    fs::write(&file_path, "").unwrap();

    let hash = compute_file_sha256(&file_path).unwrap();
    // SHA256 of empty string
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn compute_tree_hash_deterministic() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();

    let hash1 = compute_tree_hash(dir, &[]).unwrap();
    let hash2 = compute_tree_hash(dir, &[]).unwrap();
    assert_eq!(hash1, hash2);
}

#[test]
fn compute_tree_hash_changes_with_content() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("a.txt"), "aaa").unwrap();
    let hash1 = compute_tree_hash(dir, &[]).unwrap();

    fs::write(dir.join("a.txt"), "bbb").unwrap();
    let hash2 = compute_tree_hash(dir, &[]).unwrap();

    assert_ne!(hash1, hash2);
}

#[test]
fn compute_tree_hash_changes_with_filename() {
    let tmp1 = TempDir::new().unwrap();
    let dir1 = tmp1.path();
    fs::write(dir1.join("a.txt"), "content").unwrap();

    let tmp2 = TempDir::new().unwrap();
    let dir2 = tmp2.path();
    fs::write(dir2.join("b.txt"), "content").unwrap();

    let hash1 = compute_tree_hash(dir1, &[]).unwrap();
    let hash2 = compute_tree_hash(dir2, &[]).unwrap();

    assert_ne!(hash1, hash2);
}

#[test]
fn compute_tree_hash_excludes_files() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.bak"), "bak").unwrap();

    let excludes = vec!["**/*.bak".to_string()];
    let hash_with_exclude = compute_tree_hash(dir, &excludes).unwrap();

    // Compare with a dir that only has a.txt
    let tmp2 = TempDir::new().unwrap();
    let dir2 = tmp2.path();
    fs::write(dir2.join("a.txt"), "aaa").unwrap();

    let hash_without_bak = compute_tree_hash(dir2, &[]).unwrap();
    assert_eq!(hash_with_exclude, hash_without_bak);
}

#[test]
fn compute_tree_hash_sorted_order() {
    // Create files in reverse order, tree_hash should be the same regardless of creation order
    let tmp1 = TempDir::new().unwrap();
    let dir1 = tmp1.path();
    fs::write(dir1.join("z.txt"), "z").unwrap();
    fs::write(dir1.join("a.txt"), "a").unwrap();

    let tmp2 = TempDir::new().unwrap();
    let dir2 = tmp2.path();
    fs::write(dir2.join("a.txt"), "a").unwrap();
    fs::write(dir2.join("z.txt"), "z").unwrap();

    let hash1 = compute_tree_hash(dir1, &[]).unwrap();
    let hash2 = compute_tree_hash(dir2, &[]).unwrap();
    assert_eq!(hash1, hash2);
}
