use std::path::{Path, PathBuf};

use crate::{KicadError, Result};

pub struct ProjectFiles {
    pub project_dir: PathBuf,
    pub pro_file: PathBuf,
    pub pcb_file: PathBuf,
    pub sch_file: PathBuf,
}

/// Recursively find all `.boardflow.yml` files under `workspace`.
pub fn find_boardflow_ymls(workspace: &Path) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    for entry in walkdir::WalkDir::new(workspace)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && entry.file_name() == ".boardflow.yml" {
            results.push(entry.into_path());
        }
    }
    if results.is_empty() {
        return Err(KicadError::NoBoardflowYml);
    }
    Ok(results)
}

/// Detect project directories by finding `.boardflow.yml` files and returning their parent dirs.
pub fn detect_projects(workspace: &Path) -> Result<Vec<PathBuf>> {
    let ymls = find_boardflow_ymls(workspace)?;
    let dirs: Vec<PathBuf> = ymls
        .into_iter()
        .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
        .collect();
    Ok(dirs)
}

/// Find a unique `.kicad_pro` file in the given directory (non-recursive).
pub fn resolve_kicad_pro(dir: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "kicad_pro" {
                    found.push(path);
                }
            }
        }
    }
    match found.len() {
        0 => Err(KicadError::NoKicadPro(dir.to_path_buf())),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => Err(KicadError::MultipleKicadPro(dir.to_path_buf())),
    }
}

/// Resolve the `.kicad_pcb` file with the given stem in `dir`.
pub fn resolve_pcb_file(dir: &Path, stem: &str) -> Result<PathBuf> {
    let expected = dir.join(format!("{stem}.kicad_pcb"));
    if expected.is_file() {
        return Ok(expected);
    }
    // Fallback: only allow if exactly one .kicad_pcb exists in the directory
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "kicad_pcb" {
                    found.push(path);
                }
            }
        }
    }
    match found.len() {
        0 => Err(KicadError::NoKicadPcb {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
        }),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => Err(KicadError::MultipleKicadPcb {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
        }),
    }
}

/// Resolve the root `.kicad_sch` file with the given stem in `dir`.
pub fn resolve_root_schematic(dir: &Path, stem: &str) -> Result<PathBuf> {
    let expected = dir.join(format!("{stem}.kicad_sch"));
    if expected.is_file() {
        return Ok(expected);
    }
    // Fallback: only allow if exactly one .kicad_sch exists in the directory
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "kicad_sch" {
                    found.push(path);
                }
            }
        }
    }
    match found.len() {
        0 => Err(KicadError::NoKicadSch {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
        }),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => Err(KicadError::MultipleKicadSch {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
        }),
    }
}

/// Resolve all project files (.kicad_pro, .kicad_pcb, .kicad_sch) from a directory.
pub fn resolve_project_files(dir: &Path) -> Result<ProjectFiles> {
    let pro_file = resolve_kicad_pro(dir)?;
    let stem = pro_file
        .file_stem()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_string();
    let pcb_file = resolve_pcb_file(dir, &stem)?;
    let sch_file = resolve_root_schematic(dir, &stem)?;
    Ok(ProjectFiles {
        project_dir: dir.to_path_buf(),
        pro_file,
        pcb_file,
        sch_file,
    })
}
