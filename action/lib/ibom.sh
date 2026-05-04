#!/bin/bash
# ibom.sh - InteractiveHtmlBom generation

# Generate Interactive HTML BOM
# Failure is non-fatal: recorded as artifact status=failed, BoardRun continues
run_ibom() {
  local pcb_path="$1"
  local output_dir="$2"

  xvfb-run generate_interactive_bom \
    --no-browser \
    --dest-dir "$output_dir" \
    "$pcb_path" 2>&1

  local exit_code=$?
  if [ $exit_code -ne 0 ]; then
    echo "iBOM generation failed with exit code $exit_code" >&2
    return 1
  fi
  return 0
}
