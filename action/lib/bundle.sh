#!/bin/bash
# bundle.sh - Manifest/zip bundle creation

# Add artifact entry for available artifact
add_artifact_available() {
  local artifacts_json="$1"
  local type="$2"
  local path="$3"
  local content_type="$4"
  local file_path="$5"

  local sha256 size_bytes
  sha256=$(sha256sum "$file_path" | cut -d' ' -f1)
  size_bytes=$(stat -c%s "$file_path")

  echo "$artifacts_json" | jq \
    --arg type "$type" \
    --arg path "$path" \
    --arg ct "$content_type" \
    --arg sha "sha256:$sha256" \
    --argjson size "$size_bytes" \
    '. + [{"type": $type, "status": "available", "path": $path, "content_type": $ct, "sha256": $sha, "size_bytes": $size}]'
}

# Add artifact entry for failed artifact
add_artifact_failed() {
  local artifacts_json="$1"
  local type="$2"
  local error_message="$3"

  echo "$artifacts_json" | jq \
    --arg type "$type" \
    --arg msg "$error_message" \
    '. + [{"type": $type, "status": "failed", "error_message": $msg}]'
}

# Create fabrication.zip from gerber + drill files
create_fabrication_zip() {
  local gerber_dir="$1"
  local drill_dir="$2"
  local output_path="$3"

  local tmp_dir
  tmp_dir=$(mktemp -d)
  cp "$gerber_dir"/* "$tmp_dir/" 2>/dev/null
  cp "$drill_dir"/* "$tmp_dir/" 2>/dev/null

  (cd "$tmp_dir" && zip -q "$output_path" ./* 2>/dev/null)
  local exit_code=$?
  rm -rf "$tmp_dir"
  return $exit_code
}

# Generate file hashes JSON: {files: [{path, sha256}...]}
generate_file_hashes_json() {
  local project_dir="$1"
  local excludes="$2"
  local output_path="$3"

  local files_json="[]"
  while IFS= read -r rel_path; do
    [ -z "$rel_path" ] && continue
    local file_hash
    file_hash=$(compute_file_sha256 "$project_dir/$rel_path")
    files_json=$(echo "$files_json" | jq --arg p "$rel_path" --arg h "$file_hash" \
      '. + [{"path": $p, "sha256": $h}]')
  done < <(list_project_files "$project_dir" "$excludes" | LC_ALL=C sort)

  jq -n --argjson files "$files_json" '{files: $files}' > "$output_path"
}

# Generate BOM summary JSON from CSV (C-3: use sys.argv for paths)
generate_bom_summary_json() {
  local bom_csv_path="$1"
  local output_path="$2"

  if [ ! -f "$bom_csv_path" ]; then
    echo '{"components":[],"total_count":0}' > "$output_path"
    return 0
  fi

  python3 - "$bom_csv_path" "$output_path" <<'PYTHON'
import csv, json, sys
bom_csv_path = sys.argv[1]
output_path = sys.argv[2]
components = []
try:
    with open(bom_csv_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            components.append({
                'reference': row.get('Reference', row.get('Refs', '')),
                'value': row.get('Value', ''),
                'footprint': row.get('Footprint', ''),
                'quantity': row.get('Qty', '1')
            })
except Exception as e:
    print(str(e), file=sys.stderr)
result = {'components': components, 'total_count': len(components)}
with open(output_path, 'w') as f:
    json.dump(result, f, indent=2)
PYTHON
}

# Generate checks summary JSON from ERC/DRC
generate_checks_summary_json() {
  local erc_json="$1"
  local drc_json="$2"
  local output_path="$3"

  local erc_errors=0
  local erc_warnings=0
  local drc_errors=0
  local drc_warnings=0

  if [ -f "$erc_json" ]; then
    erc_errors=$(jq '[.violations[]? | select(.severity == "error")] | length' "$erc_json" 2>/dev/null || echo 0)
    erc_warnings=$(jq '[.violations[]? | select(.severity == "warning")] | length' "$erc_json" 2>/dev/null || echo 0)
  fi

  if [ -f "$drc_json" ]; then
    drc_errors=$(jq '[.violations[]? | select(.severity == "error")] | length' "$drc_json" 2>/dev/null || echo 0)
    drc_warnings=$(jq '[.violations[]? | select(.severity == "warning")] | length' "$drc_json" 2>/dev/null || echo 0)
  fi

  jq -n \
    --argjson erc_errors "$erc_errors" \
    --argjson erc_warnings "$erc_warnings" \
    --argjson drc_errors "$drc_errors" \
    --argjson drc_warnings "$drc_warnings" \
    '{erc: {errors: $erc_errors, warnings: $erc_warnings}, drc: {errors: $drc_errors, warnings: $drc_warnings}}' > "$output_path"
}

# Generate artifacts summary JSON
generate_artifacts_summary_json() {
  local artifacts_status="$1"
  local output_path="$2"

  echo "$artifacts_status" | jq '{artifacts: .}' > "$output_path"
}

# Generate previews JSON
generate_previews_json() {
  local output_dir="$1"
  local output_path="$2"

  local previews="[]"
  for f in "$output_dir/svg/pcb_top.svg" "$output_dir/svg/pcb_bottom.svg" \
           "$output_dir/3d/top.png" "$output_dir/3d/bottom.png"; do
    if [ -f "$f" ]; then
      local name
      name=$(basename "$f")
      previews=$(echo "$previews" | jq --arg name "$name" --arg path "$f" \
        '. + [{"name": $name, "path": $path}]')
    fi
  done

  jq -n --argjson previews "$previews" '{previews: $previews}' > "$output_path"
}

# Create manifest.json (M-4: spec section 8.5 compliant)
create_manifest() {
  local board_project_id="$1"
  local project_path="$2"
  local project_dir_rel="$3"
  local config_path="$4"
  local tree_hash="$5"
  local sha="$6"
  local ref="$7"
  local branch="$8"
  local run_id="$9"
  local run_attempt="${10}"
  local checks_path="${11}"
  local artifacts_status="${12}"
  local diff_dir="${13}"
  local output_path="${14}"

  local checks="{}"
  if [ -f "$checks_path" ]; then
    checks=$(cat "$checks_path")
  fi

  # Build diff_metadata
  local diff_metadata="{}"
  if [ -d "$diff_dir" ]; then
    diff_metadata=$(python3 - "$diff_dir" <<'PYTHON'
import json, sys, os, hashlib
diff_dir = sys.argv[1]
result = {}
for name in ["file_hashes", "bom_summary", "checks_summary", "artifacts_summary", "previews"]:
    path = os.path.join(diff_dir, f"{name}.json")
    if os.path.exists(path):
        size = os.path.getsize(path)
        with open(path, 'rb') as f:
            sha = hashlib.sha256(f.read()).hexdigest()
        result[name] = {"path": f"diff/{name}.json", "sha256": f"sha256:{sha}", "size_bytes": size}
print(json.dumps(result))
PYTHON
    )
  fi

  # Build artifacts array from artifacts_status
  local artifacts_array="[]"
  if [ -n "$artifacts_status" ] && [ "$artifacts_status" != "[]" ]; then
    artifacts_array="$artifacts_status"
  fi

  jq -n \
    --argjson schema_version 1 \
    --arg board_project_id "$board_project_id" \
    --arg project_path "$project_path" \
    --arg project_dir "$project_dir_rel" \
    --arg config_path "$config_path" \
    --arg ref "$ref" \
    --arg branch "$branch" \
    --arg commit_sha "$sha" \
    --arg run_id "$run_id" \
    --arg run_attempt "$run_attempt" \
    --arg tree_hash "sha256:$tree_hash" \
    --argjson checks "$checks" \
    --argjson diff_metadata "$diff_metadata" \
    --argjson artifacts "$artifacts_array" \
    '{
      schema_version: $schema_version,
      board_project_id: $board_project_id,
      project: {project_path: $project_path, project_dir: $project_dir, config_path: $config_path},
      git: {ref: $ref, branch: $branch, commit_sha: $commit_sha},
      github_actions: {run_id: $run_id, run_attempt: $run_attempt},
      kicad: {version: "9.0.x"},
      hash: {tree_hash: $tree_hash},
      diff_metadata: $diff_metadata,
      checks: $checks,
      artifacts: $artifacts
    }' > "$output_path"
}

# Create bundle zip from staging directory
create_bundle_zip() {
  local staging_dir="$1"
  local output_path="$2"

  (cd "$staging_dir" && zip -qr "$output_path" . 2>/dev/null)
}

# Compute SHA256 of bundle
compute_bundle_sha256() {
  local bundle_path="$1"
  sha256sum "$bundle_path" | cut -d' ' -f1
}
