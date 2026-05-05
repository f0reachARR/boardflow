use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    KicadPro,
    KicadSch,
    KicadPcb,
    KicadWks,
    SchematicPdf,
    PcbPdf,
    PcbTopSvg,
    PcbBottomSvg,
    RenderTopPng,
    RenderBottomPng,
    Ibom,
    BomCsv,
    PositionCsv,
    GerberZip,
    DrillZip,
    FabricationZip,
    ErcReport,
    DrcReport,
}

impl ArtifactType {
    pub const KICANVAS_REQUIRED: [Self; 3] = [Self::KicadPro, Self::KicadSch, Self::KicadPcb];
    pub const PCB_PREVIEW_REQUIRED: [Self; 2] = [Self::PcbTopSvg, Self::PcbBottomSvg];
    pub const FABRICATION_REQUIRED: [Self; 2] = [Self::GerberZip, Self::DrillZip];

    pub fn is_iframe_artifact(self) -> bool {
        matches!(self, Self::Ibom)
    }

    pub fn is_inline_display(self) -> bool {
        matches!(
            self,
            Self::Ibom | Self::SchematicPdf | Self::PcbPdf | Self::PcbTopSvg | Self::PcbBottomSvg
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::KicadPro => "kicad_pro",
            Self::KicadSch => "kicad_sch",
            Self::KicadPcb => "kicad_pcb",
            Self::KicadWks => "kicad_wks",
            Self::SchematicPdf => "schematic_pdf",
            Self::PcbPdf => "pcb_pdf",
            Self::PcbTopSvg => "pcb_top_svg",
            Self::PcbBottomSvg => "pcb_bottom_svg",
            Self::RenderTopPng => "render_top_png",
            Self::RenderBottomPng => "render_bottom_png",
            Self::Ibom => "ibom",
            Self::BomCsv => "bom_csv",
            Self::PositionCsv => "position_csv",
            Self::GerberZip => "gerber_zip",
            Self::DrillZip => "drill_zip",
            Self::FabricationZip => "fabrication_zip",
            Self::ErcReport => "erc_report",
            Self::DrcReport => "drc_report",
        }
    }
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ArtifactStatus {
    Available,
    Missing,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub r#type: ArtifactType,
    pub status: ArtifactStatus,
    pub filename: Option<String>,
    pub source_path: Option<String>,
    pub logical_name: Option<String>,
    pub content_type: Option<String>,
    pub storage_key: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub status_reason: Option<String>,
    pub error_message: Option<String>,
    pub source_bundle_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
