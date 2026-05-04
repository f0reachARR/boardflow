use std::fs;

use boardflow_kicad::detect::{
    detect_projects, find_boardflow_ymls, resolve_kicad_pro, resolve_pcb_file,
    resolve_project_files, resolve_root_schematic,
};
use tempfile::TempDir;

fn create_project_dir(dir: &std::path::Path, name: &str) {
    fs::write(dir.join(".boardflow.yml"), "version: 1\n").unwrap();
    fs::write(dir.join(format!("{name}.kicad_pro")), "{}").unwrap();
    fs::write(dir.join(format!("{name}.kicad_pcb")), "").unwrap();
    fs::write(dir.join(format!("{name}.kicad_sch")), "").unwrap();
}

#[test]
fn find_boardflow_ymls_in_nested_dirs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Create two project directories
    let proj1 = root.join("proj1");
    fs::create_dir_all(&proj1).unwrap();
    fs::write(proj1.join(".boardflow.yml"), "version: 1\n").unwrap();

    let proj2 = root.join("sub/proj2");
    fs::create_dir_all(&proj2).unwrap();
    fs::write(proj2.join(".boardflow.yml"), "version: 1\n").unwrap();

    let ymls = find_boardflow_ymls(root).unwrap();
    assert_eq!(ymls.len(), 2);
}

#[test]
fn find_boardflow_ymls_returns_error_when_none() {
    let tmp = TempDir::new().unwrap();
    let result = find_boardflow_ymls(tmp.path());
    assert!(result.is_err());
}

#[test]
fn detect_projects_returns_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let proj = root.join("myproject");
    fs::create_dir_all(&proj).unwrap();
    fs::write(proj.join(".boardflow.yml"), "version: 1\n").unwrap();

    let dirs = detect_projects(root).unwrap();
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0], proj);
}

#[test]
fn resolve_kicad_pro_single_file() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Board.kicad_pro"), "{}").unwrap();

    let pro = resolve_kicad_pro(dir).unwrap();
    assert_eq!(pro.file_name().unwrap(), "Board.kicad_pro");
}

#[test]
fn resolve_kicad_pro_no_file_returns_error() {
    let tmp = TempDir::new().unwrap();
    let result = resolve_kicad_pro(tmp.path());
    assert!(result.is_err());
}

#[test]
fn resolve_kicad_pro_multiple_files_returns_error() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("A.kicad_pro"), "{}").unwrap();
    fs::write(dir.join("B.kicad_pro"), "{}").unwrap();

    let result = resolve_kicad_pro(dir);
    assert!(result.is_err());
}

#[test]
fn resolve_pcb_file_exact_stem() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Board.kicad_pcb"), "").unwrap();

    let pcb = resolve_pcb_file(dir, "Board").unwrap();
    assert_eq!(pcb.file_name().unwrap(), "Board.kicad_pcb");
}

#[test]
fn resolve_pcb_file_fallback_different_name() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Other.kicad_pcb"), "").unwrap();

    let pcb = resolve_pcb_file(dir, "Board").unwrap();
    assert_eq!(pcb.file_name().unwrap(), "Other.kicad_pcb");
}

#[test]
fn resolve_pcb_file_not_found() {
    let tmp = TempDir::new().unwrap();
    let result = resolve_pcb_file(tmp.path(), "Board");
    assert!(result.is_err());
}

#[test]
fn resolve_root_schematic_exact_stem() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Board.kicad_sch"), "").unwrap();

    let sch = resolve_root_schematic(dir, "Board").unwrap();
    assert_eq!(sch.file_name().unwrap(), "Board.kicad_sch");
}

#[test]
fn resolve_root_schematic_fallback() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Other.kicad_sch"), "").unwrap();

    let sch = resolve_root_schematic(dir, "Board").unwrap();
    assert_eq!(sch.file_name().unwrap(), "Other.kicad_sch");
}

#[test]
fn resolve_root_schematic_not_found() {
    let tmp = TempDir::new().unwrap();
    let result = resolve_root_schematic(tmp.path(), "Board");
    assert!(result.is_err());
}

#[test]
fn resolve_project_files_full_project() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    create_project_dir(dir, "MyBoard");

    let pf = resolve_project_files(dir).unwrap();
    assert_eq!(pf.project_dir, dir);
    assert_eq!(pf.pro_file.file_name().unwrap(), "MyBoard.kicad_pro");
    assert_eq!(pf.pcb_file.file_name().unwrap(), "MyBoard.kicad_pcb");
    assert_eq!(pf.sch_file.file_name().unwrap(), "MyBoard.kicad_sch");
}

#[test]
fn resolve_project_files_missing_pcb() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("Board.kicad_pro"), "{}").unwrap();
    fs::write(dir.join("Board.kicad_sch"), "").unwrap();
    // No .kicad_pcb

    let result = resolve_project_files(dir);
    assert!(result.is_err());
}
