#!/bin/bash
# detect.sh - .boardflow.yml detection and BoardProject candidate resolution

WORKSPACE="${WORKSPACE:-/github/workspace}"

# Find all .boardflow.yml files in the workspace
find_boardflow_ymls() {
  find "$WORKSPACE" -name ".boardflow.yml" -type f 2>/dev/null
}

# Detect projects and populate PROJECTS array
detect_projects() {
  PROJECTS=()
  while IFS= read -r yml; do
    [ -z "$yml" ] && continue
    PROJECTS+=("$(dirname "$yml")")
  done < <(find_boardflow_ymls)
}

# Resolve unique .kicad_pro in directory
resolve_kicad_pro() {
  local dir="$1"
  local pros=()
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    pros+=("$f")
  done < <(find "$dir" -maxdepth 1 -name "*.kicad_pro" -type f 2>/dev/null)

  if [ ${#pros[@]} -eq 0 ]; then
    echo "No .kicad_pro found in $dir" >&2
    return 1
  fi
  if [ ${#pros[@]} -gt 1 ]; then
    echo "Multiple .kicad_pro found in $dir" >&2
    return 1
  fi
  echo "${pros[0]}"
  return 0
}

# Resolve PCB file: prefer same stem, fallback to unique .kicad_pcb
resolve_pcb_file() {
  local dir="$1"
  local pro_stem="$2"
  local same_stem="$dir/${pro_stem}.kicad_pcb"

  if [ -f "$same_stem" ]; then
    echo "$same_stem"
    return 0
  fi

  local pcbs=()
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    pcbs+=("$f")
  done < <(find "$dir" -maxdepth 1 -name "*.kicad_pcb" -type f 2>/dev/null)

  if [ ${#pcbs[@]} -eq 1 ]; then
    echo "${pcbs[0]}"
    return 0
  fi

  echo "Cannot resolve unique .kicad_pcb in $dir" >&2
  return 1
}

# Resolve root schematic: prefer same stem, fallback to unique .kicad_sch
resolve_root_schematic() {
  local dir="$1"
  local pro_stem="$2"
  local same_stem="$dir/${pro_stem}.kicad_sch"

  if [ -f "$same_stem" ]; then
    echo "$same_stem"
    return 0
  fi

  local schs=()
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    schs+=("$f")
  done < <(find "$dir" -maxdepth 1 -name "*.kicad_sch" -type f 2>/dev/null)

  if [ ${#schs[@]} -eq 1 ]; then
    echo "${schs[0]}"
    return 0
  fi

  echo "Cannot resolve unique .kicad_sch in $dir" >&2
  return 1
}

# Validate that required files are not excluded
validate_required_files() {
  local dir="$1"
  local pro="$2"
  local pcb="$3"
  local sch="$4"
  local excludes="$5"

  for file in "$pro" "$pcb" "$sch"; do
    local rel="${file#$dir/}"
    if is_excluded "$rel" "$excludes"; then
      echo "Required file $rel is excluded" >&2
      return 1
    fi
  done
  return 0
}
