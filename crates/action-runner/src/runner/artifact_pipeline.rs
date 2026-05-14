use std::fs;
use std::path::Path;

use boardflow_domain::models::artifact::ArtifactType;
use boardflow_kicad::cli::{KicadCli, PcbSide};
use boardflow_kicad::hash;
use serde::Serialize;
use tracing::warn;

use crate::bundle;
use crate::error::ActionError;
use crate::inputs::ActionInputs;

use super::project_discovery::ValidProject;

#[derive(Serialize)]
pub(super) struct ArtifactEntry {
    #[serde(rename = "type")]
    artifact_type: ArtifactType,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

impl ArtifactEntry {
    fn available(
        artifact_type: ArtifactType,
        path: &str,
        content_type: &str,
        file_path: &Path,
    ) -> Self {
        let sha256 = hash::compute_file_sha256(file_path).ok();
        let size_bytes = fs::metadata(file_path).ok().map(|m| m.len());
        Self {
            artifact_type,
            status: "available".to_string(),
            path: Some(path.to_string()),
            source_path: None,
            content_type: Some(content_type.to_string()),
            sha256: sha256.map(|h| format!("sha256:{h}")),
            size_bytes,
            error_message: None,
        }
    }

    fn failed(artifact_type: ArtifactType, message: &str) -> Self {
        Self {
            artifact_type,
            status: "failed".to_string(),
            path: None,
            source_path: None,
            content_type: None,
            sha256: None,
            size_bytes: None,
            error_message: Some(message.to_string()),
        }
    }

    fn source(
        artifact_type: ArtifactType,
        staging_path: &str,
        source_path: &str,
        file_path: &Path,
    ) -> Self {
        let sha256 = hash::compute_file_sha256(file_path).ok();
        let size_bytes = fs::metadata(file_path).ok().map(|m| m.len());
        Self {
            artifact_type,
            status: "available".to_string(),
            path: Some(staging_path.to_string()),
            source_path: Some(source_path.to_string()),
            content_type: None,
            sha256: sha256.map(|h| format!("sha256:{h}")),
            size_bytes,
            error_message: None,
        }
    }
}

pub(super) async fn run_artifact_pipeline(
    kicad: &KicadCli,
    vp: &ValidProject,
    inputs: &ActionInputs,
    output_path: &Path,
) -> std::result::Result<(Vec<serde_json::Value>, bool), ActionError> {
    let mut artifacts: Vec<serde_json::Value> = Vec::new();
    let mut checks_failed = false;

    // Run ERC
    let erc_json = output_path.join("erc.json");
    match kicad.run_erc(&vp.sch_file, &erc_json).await {
        Ok(cmd_out) => {
            if cmd_out.exit_code == 0 || cmd_out.exit_code == 5 {
                artifacts.push(
                    serde_json::to_value(ArtifactEntry::available(
                        ArtifactType::ErcReport,
                        "checks/erc.json",
                        "application/json",
                        &erc_json,
                    ))
                    .unwrap(),
                );
                if cmd_out.exit_code == 5 && inputs.fail_on_erc {
                    checks_failed = true;
                }
            } else {
                artifacts.push(
                    serde_json::to_value(ArtifactEntry::failed(
                        ArtifactType::ErcReport,
                        "ERC execution failed",
                    ))
                    .unwrap(),
                );
            }
        }
        Err(e) => {
            warn!("ERC failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::ErcReport,
                    &format!("ERC execution failed: {e}"),
                ))
                .unwrap(),
            );
        }
    }

    // Run DRC
    let drc_json = output_path.join("drc.json");
    match kicad.run_drc(&vp.pcb_file, &drc_json).await {
        Ok(cmd_out) => {
            if cmd_out.exit_code == 0 || cmd_out.exit_code == 5 {
                artifacts.push(
                    serde_json::to_value(ArtifactEntry::available(
                        ArtifactType::DrcReport,
                        "checks/drc.json",
                        "application/json",
                        &drc_json,
                    ))
                    .unwrap(),
                );
                if cmd_out.exit_code == 5 && inputs.fail_on_drc {
                    checks_failed = true;
                }
            } else {
                artifacts.push(
                    serde_json::to_value(ArtifactEntry::failed(
                        ArtifactType::DrcReport,
                        "DRC execution failed",
                    ))
                    .unwrap(),
                );
            }
        }
        Err(e) => {
            warn!("DRC failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::DrcReport,
                    &format!("DRC execution failed: {e}"),
                ))
                .unwrap(),
            );
        }
    }

    // Export PCB PDF
    let pdf_dir = output_path.join("pdf");
    fs::create_dir_all(&pdf_dir)?;
    match kicad
        .export_pcb_pdf(&vp.pcb_file, &pdf_dir.join("pcb.pdf"))
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::PcbPdf,
                    "review/pcb.pdf",
                    "application/pdf",
                    &pdf_dir.join("pcb.pdf"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("PCB PDF failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::PcbPdf,
                    "PCB PDF export failed",
                ))
                .unwrap(),
            );
        }
    }

    // Export Schematic PDF
    match kicad
        .export_sch_pdf(&vp.sch_file, &pdf_dir.join("schematic.pdf"))
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::SchematicPdf,
                    "review/schematic.pdf",
                    "application/pdf",
                    &pdf_dir.join("schematic.pdf"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("Schematic PDF failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::SchematicPdf,
                    "Schematic PDF export failed",
                ))
                .unwrap(),
            );
        }
    }

    // Export SVG
    let svg_dir = output_path.join("svg");
    fs::create_dir_all(&svg_dir)?;
    match kicad
        .export_pcb_svg(&vp.pcb_file, &svg_dir.join("pcb_top.svg"), PcbSide::Top)
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::PcbTopSvg,
                    "review/pcb_top.svg",
                    "image/svg+xml",
                    &svg_dir.join("pcb_top.svg"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("PCB top SVG failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::PcbTopSvg,
                    "PCB top SVG export failed",
                ))
                .unwrap(),
            );
        }
    }

    match kicad
        .export_pcb_svg(
            &vp.pcb_file,
            &svg_dir.join("pcb_bottom.svg"),
            PcbSide::Bottom,
        )
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::PcbBottomSvg,
                    "review/pcb_bottom.svg",
                    "image/svg+xml",
                    &svg_dir.join("pcb_bottom.svg"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("PCB bottom SVG failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::PcbBottomSvg,
                    "PCB bottom SVG export failed",
                ))
                .unwrap(),
            );
        }
    }

    // Export Gerber
    let gerber_dir = output_path.join("gerber");
    fs::create_dir_all(&gerber_dir)?;
    let gerber_ok = kicad
        .export_gerbers(&vp.pcb_file, &gerber_dir)
        .await
        .is_ok();

    // Export Drill
    let drill_dir = output_path.join("drill");
    fs::create_dir_all(&drill_dir)?;
    let drill_ok = kicad.export_drill(&vp.pcb_file, &drill_dir).await.is_ok();

    // Create zip archives
    let gerbers_zip = output_path.join("gerbers.zip");
    if gerber_ok {
        if bundle::create_bundle_zip(&gerber_dir, &gerbers_zip).is_ok() {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::GerberZip,
                    "fabrication/gerbers.zip",
                    "application/zip",
                    &gerbers_zip,
                ))
                .unwrap(),
            );
        } else {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::GerberZip,
                    "Gerber zip creation failed",
                ))
                .unwrap(),
            );
        }
    } else {
        artifacts.push(
            serde_json::to_value(ArtifactEntry::failed(
                ArtifactType::GerberZip,
                "Gerber export failed",
            ))
            .unwrap(),
        );
    }

    let drill_zip = output_path.join("drill.zip");
    if drill_ok {
        if bundle::create_bundle_zip(&drill_dir, &drill_zip).is_ok() {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::DrillZip,
                    "fabrication/drill.zip",
                    "application/zip",
                    &drill_zip,
                ))
                .unwrap(),
            );
        } else {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::DrillZip,
                    "Drill zip creation failed",
                ))
                .unwrap(),
            );
        }
    } else {
        artifacts.push(
            serde_json::to_value(ArtifactEntry::failed(
                ArtifactType::DrillZip,
                "Drill export failed",
            ))
            .unwrap(),
        );
    }

    // Fabrication zip (combined)
    let fab_zip = output_path.join("fabrication.zip");
    if gerber_ok || drill_ok {
        if bundle::create_fabrication_zip(&gerber_dir, &drill_dir, &fab_zip).is_ok() {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::FabricationZip,
                    "fabrication/fabrication.zip",
                    "application/zip",
                    &fab_zip,
                ))
                .unwrap(),
            );
        } else {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::FabricationZip,
                    "Fabrication zip creation failed",
                ))
                .unwrap(),
            );
        }
    } else {
        artifacts.push(
            serde_json::to_value(ArtifactEntry::failed(
                ArtifactType::FabricationZip,
                "Fabrication zip creation failed",
            ))
            .unwrap(),
        );
    }

    // Export BOM
    let bom_dir = output_path.join("bom");
    fs::create_dir_all(&bom_dir)?;
    match kicad
        .export_bom(&vp.sch_file, &bom_dir.join("bom.csv"))
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::BomCsv,
                    "assembly/bom.csv",
                    "text/csv",
                    &bom_dir.join("bom.csv"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("BOM export failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::BomCsv,
                    "BOM export failed",
                ))
                .unwrap(),
            );
        }
    }

    // Export Position
    let pos_dir = output_path.join("position");
    fs::create_dir_all(&pos_dir)?;
    match kicad
        .export_position(&vp.pcb_file, &pos_dir.join("position.csv"))
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::PositionCsv,
                    "assembly/position.csv",
                    "text/csv",
                    &pos_dir.join("position.csv"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("Position export failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::PositionCsv,
                    "Position export failed",
                ))
                .unwrap(),
            );
        }
    }

    // 3D Renders
    let render_dir = output_path.join("3d");
    fs::create_dir_all(&render_dir)?;
    match kicad
        .render_3d(&vp.pcb_file, &render_dir.join("top.png"), PcbSide::Top)
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::RenderTopPng,
                    "review/render_top.png",
                    "image/png",
                    &render_dir.join("top.png"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("3D top render failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::RenderTopPng,
                    "3D top render failed",
                ))
                .unwrap(),
            );
        }
    }

    match kicad
        .render_3d(
            &vp.pcb_file,
            &render_dir.join("bottom.png"),
            PcbSide::Bottom,
        )
        .await
    {
        Ok(_) => {
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::RenderBottomPng,
                    "review/render_bottom.png",
                    "image/png",
                    &render_dir.join("bottom.png"),
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("3D bottom render failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::RenderBottomPng,
                    "3D bottom render failed",
                ))
                .unwrap(),
            );
        }
    }

    // iBOM
    let ibom_dir = output_path.join("ibom");
    fs::create_dir_all(&ibom_dir)?;
    match boardflow_kicad::ibom::run_ibom(&vp.pcb_file, &ibom_dir).await {
        Ok(html_path) => {
            // Copy to expected staging name
            let dest = ibom_dir.join("ibom.html");
            if html_path != dest {
                let _ = fs::copy(&html_path, &dest);
            }
            artifacts.push(
                serde_json::to_value(ArtifactEntry::available(
                    ArtifactType::Ibom,
                    "assembly/ibom.html",
                    "text/html",
                    &dest,
                ))
                .unwrap(),
            );
        }
        Err(e) => {
            warn!("iBOM failed: {e}");
            artifacts.push(
                serde_json::to_value(ArtifactEntry::failed(
                    ArtifactType::Ibom,
                    "iBOM generation failed",
                ))
                .unwrap(),
            );
        }
    }

    // KiCad source files as artifacts
    if let Ok(entries) = fs::read_dir(&vp.project_dir) {
        let mut src_entries: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path().is_file()
                    && matches!(
                        e.path().extension().and_then(|e| e.to_str()),
                        Some("kicad_pro" | "kicad_sch" | "kicad_pcb" | "kicad_wks")
                    )
            })
            .collect();
        src_entries.sort_by_key(|e| e.path());

        for entry in src_entries {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let src_rel = file_name.to_string();

            if hash::is_excluded(&src_rel, &vp.excludes) {
                continue;
            }

            let kicad_type = match path.extension().and_then(|e| e.to_str()) {
                Some("kicad_pro") => ArtifactType::KicadPro,
                Some("kicad_sch") => ArtifactType::KicadSch,
                Some("kicad_pcb") => ArtifactType::KicadPcb,
                Some("kicad_wks") => ArtifactType::KicadWks,
                _ => continue,
            };

            let (staging_path, source_path) = if vp.rel_dir == "." {
                (format!("kicad/{src_rel}"), src_rel.clone())
            } else {
                (
                    format!("kicad/{}/{src_rel}", vp.rel_dir),
                    format!("{}/{src_rel}", vp.rel_dir),
                )
            };

            artifacts.push(
                serde_json::to_value(ArtifactEntry::source(
                    kicad_type,
                    &staging_path,
                    &source_path,
                    &path,
                ))
                .unwrap(),
            );
        }
    }

    Ok((artifacts, checks_failed))
}
