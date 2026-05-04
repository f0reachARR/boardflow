#!/bin/bash
# kicad.sh - KiCad CLI execution wrappers

KICAD_TIMEOUT=300

# Run ERC check
run_erc() {
  local sch_file="$1"
  local output_json="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli sch erc \
    --format json \
    --severity-all \
    --exit-code-violations \
    --output "$output_json" \
    "$sch_file" 2>&1

  local exit_code=$?
  # exit 5 = violations found but JSON generated successfully
  if [ $exit_code -eq 0 ] || [ $exit_code -eq 5 ]; then
    return $exit_code
  fi
  echo "ERC failed with exit code $exit_code" >&2
  return $exit_code
}

# Run DRC check
run_drc() {
  local pcb_file="$1"
  local output_json="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb drc \
    --format json \
    --severity-all \
    --exit-code-violations \
    --output "$output_json" \
    "$pcb_file" 2>&1

  local exit_code=$?
  if [ $exit_code -eq 0 ] || [ $exit_code -eq 5 ]; then
    return $exit_code
  fi
  echo "DRC failed with exit code $exit_code" >&2
  return $exit_code
}

# Export PCB PDF
run_pcb_pdf() {
  local pcb_file="$1"
  local output_pdf="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export pdf \
    --layers "F.Cu,B.Cu,F.Silkscreen,B.Silkscreen,Edge.Cuts" \
    --output "$output_pdf" \
    "$pcb_file" 2>&1
}

# Export Schematic PDF
run_sch_pdf() {
  local sch_file="$1"
  local output_pdf="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli sch export pdf \
    --output "$output_pdf" \
    "$sch_file" 2>&1
}

# Export PCB SVG (top)
run_pcb_svg_top() {
  local pcb_file="$1"
  local output_svg="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export svg \
    --mode-multi \
    --layers "F.Cu,F.Silkscreen,F.Mask,Edge.Cuts" \
    --output "$output_svg" \
    "$pcb_file" 2>&1
}

# Export PCB SVG (bottom)
run_pcb_svg_bottom() {
  local pcb_file="$1"
  local output_svg="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export svg \
    --mode-multi \
    --layers "B.Cu,B.Silkscreen,B.Mask,Edge.Cuts" \
    --output "$output_svg" \
    "$pcb_file" 2>&1
}

# Export Gerbers
run_gerber_export() {
  local pcb_file="$1"
  local output_dir="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export gerbers \
    --output "$output_dir/" \
    "$pcb_file" 2>&1
}

# Export Drill files
run_drill_export() {
  local pcb_file="$1"
  local output_dir="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export drill \
    --format excellon \
    --excellon-separate-th \
    --output "$output_dir/" \
    "$pcb_file" 2>&1
}

# Export BOM
run_bom_export() {
  local sch_file="$1"
  local output_csv="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli sch export bom \
    --output "$output_csv" \
    "$sch_file" 2>&1
}

# Export Position file
run_position_export() {
  local pcb_file="$1"
  local output_csv="$2"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb export pos \
    --format csv \
    --output "$output_csv" \
    "$pcb_file" 2>&1
}

# Render 3D view
run_3d_render() {
  local pcb_file="$1"
  local output_png="$2"
  local side="$3"

  timeout "$KICAD_TIMEOUT" kicad-cli pcb render \
    --side "$side" \
    --quality basic \
    --output "$output_png" \
    "$pcb_file" 2>&1
}
