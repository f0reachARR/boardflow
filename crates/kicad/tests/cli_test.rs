use std::fs;
use std::path::Path;

use boardflow_kicad::KicadError;
use boardflow_kicad::cli::{KicadCli, PcbSide};
use tempfile::TempDir;

#[test]
fn build_erc_args_matches_bash() {
    let args = KicadCli::build_erc_args(
        Path::new("/proj/board.kicad_sch"),
        Path::new("/out/erc.json"),
    );
    assert_eq!(
        args,
        vec![
            "sch",
            "erc",
            "--format",
            "json",
            "--severity-all",
            "--exit-code-violations",
            "--output",
            "/out/erc.json",
            "/proj/board.kicad_sch",
        ]
    );
}

#[test]
fn build_drc_args_matches_bash() {
    let args = KicadCli::build_drc_args(
        Path::new("/proj/board.kicad_pcb"),
        Path::new("/out/drc.json"),
    );
    assert_eq!(
        args,
        vec![
            "pcb",
            "drc",
            "--format",
            "json",
            "--severity-all",
            "--exit-code-violations",
            "--output",
            "/out/drc.json",
            "/proj/board.kicad_pcb",
        ]
    );
}

#[test]
fn build_pcb_pdf_args_matches_bash() {
    let args =
        KicadCli::build_pcb_pdf_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/b.pdf"));
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "pdf",
            "--layers",
            "F.Cu,B.Cu,F.Silkscreen,B.Silkscreen,Edge.Cuts",
            "--output",
            "/out/b.pdf",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_sch_pdf_args_matches_bash() {
    let args =
        KicadCli::build_sch_pdf_args(Path::new("/proj/b.kicad_sch"), Path::new("/out/b.pdf"));
    assert_eq!(
        args,
        vec![
            "sch",
            "export",
            "pdf",
            "--output",
            "/out/b.pdf",
            "/proj/b.kicad_sch"
        ]
    );
}

#[test]
fn build_pcb_svg_top_args_matches_bash() {
    let args = KicadCli::build_pcb_svg_args(
        Path::new("/proj/b.kicad_pcb"),
        Path::new("/out/top.svg"),
        PcbSide::Top,
    );
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "svg",
            "--mode-multi",
            "--layers",
            "F.Cu,F.Silkscreen,F.Mask,Edge.Cuts",
            "--output",
            "/out/top.svg",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_pcb_svg_bottom_args_matches_bash() {
    let args = KicadCli::build_pcb_svg_args(
        Path::new("/proj/b.kicad_pcb"),
        Path::new("/out/bottom.svg"),
        PcbSide::Bottom,
    );
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "svg",
            "--mode-multi",
            "--layers",
            "B.Cu,B.Silkscreen,B.Mask,Edge.Cuts",
            "--output",
            "/out/bottom.svg",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_gerbers_args_matches_bash() {
    let args =
        KicadCli::build_gerbers_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/gerbers"));
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "gerbers",
            "--output",
            "/out/gerbers/",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_gerbers_args_trailing_slash() {
    // If output_dir already ends with /, no double slash
    let args =
        KicadCli::build_gerbers_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/gerbers/"));
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "gerbers",
            "--output",
            "/out/gerbers/",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_drill_args_matches_bash() {
    let args = KicadCli::build_drill_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/drill"));
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "drill",
            "--format",
            "excellon",
            "--excellon-separate-th",
            "--output",
            "/out/drill/",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_bom_args_matches_bash() {
    let args = KicadCli::build_bom_args(Path::new("/proj/b.kicad_sch"), Path::new("/out/bom.csv"));
    assert_eq!(
        args,
        vec![
            "sch",
            "export",
            "bom",
            "--output",
            "/out/bom.csv",
            "/proj/b.kicad_sch"
        ]
    );
}

#[test]
fn build_position_args_matches_bash() {
    let args =
        KicadCli::build_position_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/pos.csv"));
    assert_eq!(
        args,
        vec![
            "pcb",
            "export",
            "pos",
            "--format",
            "csv",
            "--output",
            "/out/pos.csv",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_render_3d_top_args_matches_bash() {
    let args = KicadCli::build_render_3d_args(
        Path::new("/proj/b.kicad_pcb"),
        Path::new("/out/3d_top.png"),
        PcbSide::Top,
    );
    assert_eq!(
        args,
        vec![
            "pcb",
            "render",
            "--side",
            "top",
            "--quality",
            "basic",
            "--output",
            "/out/3d_top.png",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[test]
fn build_render_3d_bottom_args_matches_bash() {
    let args = KicadCli::build_render_3d_args(
        Path::new("/proj/b.kicad_pcb"),
        Path::new("/out/3d_bottom.png"),
        PcbSide::Bottom,
    );
    assert_eq!(
        args,
        vec![
            "pcb",
            "render",
            "--side",
            "bottom",
            "--quality",
            "basic",
            "--output",
            "/out/3d_bottom.png",
            "/proj/b.kicad_pcb",
        ]
    );
}

#[cfg(unix)]
fn write_fake_kicad_cli(dir: &TempDir, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.path().join("fake-kicad-cli.sh");
    fs::write(&script_path, format!("#!/bin/sh\n{body}\n")).expect("write fake kicad script");
    let mut perms = script_path.metadata().expect("read metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("set executable bit");
    script_path
}

#[cfg(unix)]
#[tokio::test]
async fn export_pcb_pdf_requires_nonempty_output_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let script_path = write_fake_kicad_cli(&temp_dir, "exit 0");
    let cli = KicadCli::with_bin_path(script_path);
    let output = temp_dir.path().join("board.pdf");

    let err = cli
        .export_pcb_pdf(Path::new("/proj/board.kicad_pcb"), &output)
        .await
        .expect_err("missing output should fail");

    match err {
        KicadError::OutputMissing { path, .. } => assert_eq!(path, output),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn export_pcb_pdf_rejects_empty_output_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let output = temp_dir.path().join("board.pdf");
    let script_path =
        write_fake_kicad_cli(&temp_dir, &format!(": > \"{}\"\nexit 0", output.display()));
    let cli = KicadCli::with_bin_path(script_path);

    let err = cli
        .export_pcb_pdf(Path::new("/proj/board.kicad_pcb"), &output)
        .await
        .expect_err("empty output should fail");

    match err {
        KicadError::OutputEmpty { path, .. } => assert_eq!(path, output),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn run_erc_accepts_exit_code_5_when_output_file_is_nonempty() {
    let temp_dir = TempDir::new().expect("temp dir");
    let output = temp_dir.path().join("erc.json");
    let script_path = write_fake_kicad_cli(
        &temp_dir,
        &format!("printf '{{}}' > \"{}\"\nexit 5", output.display()),
    );
    let cli = KicadCli::with_bin_path(script_path);

    let result = cli
        .run_erc(Path::new("/proj/board.kicad_sch"), &output)
        .await
        .expect("non-empty ERC output should succeed");

    assert_eq!(result.exit_code, 5);
}
