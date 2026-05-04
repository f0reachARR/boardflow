use std::path::Path;

use boardflow_kicad::cli::KicadCli;

#[test]
fn build_erc_args_matches_bash() {
    let args = KicadCli::build_erc_args(Path::new("/proj/board.kicad_sch"), Path::new("/out/erc.json"));
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
    let args = KicadCli::build_drc_args(Path::new("/proj/board.kicad_pcb"), Path::new("/out/drc.json"));
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
    let args = KicadCli::build_pcb_pdf_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/b.pdf"));
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
    let args = KicadCli::build_sch_pdf_args(Path::new("/proj/b.kicad_sch"), Path::new("/out/b.pdf"));
    assert_eq!(
        args,
        vec!["sch", "export", "pdf", "--output", "/out/b.pdf", "/proj/b.kicad_sch"]
    );
}

#[test]
fn build_pcb_svg_top_args_matches_bash() {
    let args = KicadCli::build_pcb_svg_args(
        Path::new("/proj/b.kicad_pcb"),
        Path::new("/out/top.svg"),
        "top",
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
        "bottom",
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
    let args = KicadCli::build_gerbers_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/gerbers"));
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
    let args = KicadCli::build_gerbers_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/gerbers/"));
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
        vec!["sch", "export", "bom", "--output", "/out/bom.csv", "/proj/b.kicad_sch"]
    );
}

#[test]
fn build_position_args_matches_bash() {
    let args = KicadCli::build_position_args(Path::new("/proj/b.kicad_pcb"), Path::new("/out/pos.csv"));
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
        "top",
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
        "bottom",
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
