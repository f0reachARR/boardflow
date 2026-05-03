#!/bin/bash
# BoardFlow Action - Main Entrypoint
# Orchestrates KiCad artifact generation and upload to BoardFlow

# Source library scripts
for lib in /action/lib/*.sh; do
  source "$lib"
done

# Parse inputs (C-1: GitHub Actions converts hyphens to underscores)
TOKEN="${INPUT_TOKEN:-}"
MODE="${INPUT_MODE:-auto}"
EXCLUDE_PATHS="${INPUT_EXCLUDE_PATHS:-}"
API_URL="${INPUT_API_URL:-https://api.boardflow.example.com}"
FAIL_ON_DRC="${INPUT_FAIL_ON_DRC:-false}"
FAIL_ON_ERC="${INPUT_FAIL_ON_ERC:-false}"

WORKSPACE="/github/workspace"
EVENT_NAME="${GITHUB_EVENT_NAME:-}"
REPO="${GITHUB_REPOSITORY:-}"
SHA="${GITHUB_SHA:-}"
REF="${GITHUB_REF:-}"
BRANCH="${GITHUB_REF_NAME:-}"
RUN_ID="${GITHUB_RUN_ID:-}"
RUN_ATTEMPT="${GITHUB_RUN_ATTEMPT:-1}"
OWNER="${REPO%%/*}"
REPO_NAME="${REPO#*/}"

# Validate required inputs
if [ -z "$TOKEN" ]; then
  echo "::error::Input 'token' is required" >&2
  exit 1
fi

# Check unsupported events
if [ "$EVENT_NAME" = "pull_request" ]; then
  write_unsupported_event_summary "$EVENT_NAME"
  echo '{"status":"skipped","reason":"unsupported event: pull_request"}' > "$GITHUB_OUTPUT" 2>/dev/null || true
  exit 0
fi

# Detect projects
PROJECTS=()
detect_projects

if [ ${#PROJECTS[@]} -eq 0 ]; then
  echo "::error::No .boardflow.yml found in workspace" >&2
  exit 1
fi

# Validate all projects (H-4: track detection errors)
VALID_PROJECTS=()
DETECTION_ERRORS=0

for project_dir in "${PROJECTS[@]}"; do
  yml_path="$project_dir/.boardflow.yml"
  config_json=$(parse_boardflow_yml "$yml_path")
  if [ $? -ne 0 ]; then
    echo "::warning::Failed to parse $yml_path" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  if ! validate_schema_v1 "$config_json"; then
    echo "::warning::Invalid schema in $yml_path" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  yml_excludes=$(get_exclude_paths_from_config "$config_json")
  all_excludes=$(merge_excludes "$BUILTIN_EXCLUDES" "$EXCLUDE_PATHS" "$yml_excludes")

  pro_file=$(resolve_kicad_pro "$project_dir")
  if [ $? -ne 0 ] || [ -z "$pro_file" ]; then
    echo "::warning::No unique .kicad_pro in $project_dir" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  pro_stem=$(basename "$pro_file" .kicad_pro)
  pcb_file=$(resolve_pcb_file "$project_dir" "$pro_stem")
  if [ $? -ne 0 ] || [ -z "$pcb_file" ]; then
    echo "::warning::No .kicad_pcb found for $pro_stem in $project_dir" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  sch_file=$(resolve_root_schematic "$project_dir" "$pro_stem")
  if [ $? -ne 0 ] || [ -z "$sch_file" ]; then
    echo "::warning::No .kicad_sch found for $pro_stem in $project_dir" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  if ! validate_required_files "$project_dir" "$pro_file" "$pcb_file" "$sch_file" "$all_excludes"; then
    echo "::warning::Required files excluded in $project_dir" >&2
    DETECTION_ERRORS=$((DETECTION_ERRORS + 1))
    continue
  fi

  VALID_PROJECTS+=("$project_dir|$pro_file|$pcb_file|$sch_file|$all_excludes|$config_json")
done

if [ ${#VALID_PROJECTS[@]} -eq 0 ]; then
  echo "::error::No valid projects found" >&2
  exit 1
fi

# Compute hashes and build plan payload (M-6: spec compliant)
plan_projects="[]"
for entry in "${VALID_PROJECTS[@]}"; do
  IFS='|' read -r project_dir pro_file pcb_file sch_file all_excludes config_json <<< "$entry"
  tree_hash=$(compute_tree_hash "$project_dir" "$all_excludes")
  rel_dir="${project_dir#$WORKSPACE/}"
  if [ "$rel_dir" = "$project_dir" ]; then
    rel_dir="."
  fi

  # R-2: project_path is .kicad_pro relative path
  rel_pro="${pro_file#$WORKSPACE/}"
  yml_rel="$rel_dir/.boardflow.yml"

  # R-3: files array with per-file sha256
  files_json="[]"
  while IFS= read -r rel_path; do
    [ -z "$rel_path" ] && continue
    local_hash=$(compute_file_sha256 "$project_dir/$rel_path")
    files_json=$(echo "$files_json" | jq --arg p "$rel_path" --arg h "sha256:$local_hash" \
      '. + [{"path": $p, "sha256": $h}]')
  done < <(list_project_files "$project_dir" "$all_excludes" | LC_ALL=C sort)

  plan_projects=$(echo "$plan_projects" | jq \
    --arg path "$rel_pro" \
    --arg config_path "$yml_rel" \
    --arg project_dir "$rel_dir" \
    --arg hash "sha256:$tree_hash" \
    --argjson files "$files_json" \
    '. + [{"project_path": $path, "config_path": $config_path, "project_dir": $project_dir, "tree_hash": $hash, "files": $files}]')
done

# Call Plan API (M-6: spec compliant payload)
plan_payload=$(jq -n \
  --arg owner "$OWNER" \
  --arg name "$REPO_NAME" \
  --arg sha "$SHA" \
  --arg ref "$REF" \
  --arg branch "$BRANCH" \
  --arg event "$EVENT_NAME" \
  --arg mode "$MODE" \
  --arg run_id "$RUN_ID" \
  --arg run_attempt "$RUN_ATTEMPT" \
  --arg workflow "BoardFlow" \
  --argjson projects "$plan_projects" \
  '{
    repository: {owner: $owner, name: $name},
    git: {ref: $ref, branch: $branch, commit_sha: $sha, event_name: $event},
    action: {workflow: $workflow, run_id: $run_id, run_attempt: $run_attempt},
    mode: $mode,
    projects: $projects
  }')

decisions=$(call_plan_api "$plan_payload")
if [ $? -ne 0 ]; then
  echo "::error::Plan API call failed" >&2
  exit 1
fi

# Process each project based on decisions
EXIT_CODE=0
RESULTS="[]"

for i in "${!VALID_PROJECTS[@]}"; do
  entry="${VALID_PROJECTS[$i]}"
  IFS='|' read -r project_dir pro_file pcb_file sch_file all_excludes config_json <<< "$entry"

  rel_dir="${project_dir#$WORKSPACE/}"
  if [ "$rel_dir" = "$project_dir" ]; then
    rel_dir="."
  fi

  # R-2: project_path is .kicad_pro relative path
  rel_project_path="${pro_file#$WORKSPACE/}"

  decision=$(echo "$decisions" | jq -r --arg path "$rel_project_path" '.[] | select(.project_path == $path) | .decision // "skip"')

  if [ "$decision" != "build" ]; then
    RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "skipped" \
      '. + [{"path": $path, "status": $status}]')
    continue
  fi

  pro_stem=$(basename "$pro_file" .kicad_pro)
  output_dir=$(mktemp -d "/tmp/boardflow-${pro_stem}-XXXXXX")
  artifacts_status="[]"
  board_run_id=""
  upload_url=""
  staging_object_key=""

  # H-2: Get board_project_id from plan decisions
  board_project_id=$(echo "$decisions" | jq -r --arg path "$rel_project_path" '.[] | select(.project_path == $path) | .board_project_id')

  # Create board run (H-2: spec-compliant payload)
  tree_hash=$(compute_tree_hash "$project_dir" "$all_excludes")
  create_payload=$(jq -n \
    --arg board_project_id "$board_project_id" \
    --arg project_path "$rel_project_path" \
    --arg tree_hash "sha256:$tree_hash" \
    --arg commit_sha "$SHA" \
    --arg branch "$BRANCH" \
    --arg ref "$REF" \
    --arg github_run_id "$RUN_ID" \
    --arg github_run_attempt "$RUN_ATTEMPT" \
    '{board_project_id: $board_project_id, project_path: $project_path, tree_hash: $tree_hash, commit_sha: $commit_sha, branch: $branch, ref: $ref, github_run_id: $github_run_id, github_run_attempt: $github_run_attempt}')

  create_response=$(call_create_board_run "$create_payload")
  if [ $? -ne 0 ]; then
    echo "::error::Failed to create board run for $rel_project_path" >&2
    RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "error" \
      '. + [{"path": $path, "status": $status, "error": "create_board_run failed"}]')
    EXIT_CODE=1
    continue
  fi

  # C-2: Extract board_run_id, upload_url, and staging_object_key
  board_run_id=$(echo "$create_response" | jq -r '.board_run_id')
  upload_url=$(echo "$create_response" | jq -r '.artifact_bundle.upload_url')
  staging_object_key=$(echo "$create_response" | jq -r '.artifact_bundle.object_key')

  # Run ERC
  erc_json="$output_dir/erc.json"
  run_erc "$sch_file" "$erc_json"
  erc_exit=$?
  if [ $erc_exit -eq 0 ] || [ $erc_exit -eq 5 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "erc_report" "checks/erc.json" "application/json" "$erc_json")
    if [ "$erc_exit" -eq 5 ] && [ "$FAIL_ON_ERC" = "true" ]; then
      EXIT_CODE=1
    fi
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "erc_report" "ERC execution failed")
  fi

  # Run DRC
  drc_json="$output_dir/drc.json"
  run_drc "$pcb_file" "$drc_json"
  drc_exit=$?
  if [ $drc_exit -eq 0 ] || [ $drc_exit -eq 5 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "drc_report" "checks/drc.json" "application/json" "$drc_json")
    if [ "$drc_exit" -eq 5 ] && [ "$FAIL_ON_DRC" = "true" ]; then
      EXIT_CODE=1
    fi
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "drc_report" "DRC execution failed")
  fi

  # Export PCB PDF
  pdf_dir="$output_dir/pdf"
  mkdir -p "$pdf_dir"
  run_pcb_pdf "$pcb_file" "$pdf_dir/pcb.pdf"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "pcb_pdf" "review/pcb.pdf" "application/pdf" "$pdf_dir/pcb.pdf")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "pcb_pdf" "PCB PDF export failed")
  fi

  # Export Schematic PDF
  run_sch_pdf "$sch_file" "$pdf_dir/schematic.pdf"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "schematic_pdf" "review/schematic.pdf" "application/pdf" "$pdf_dir/schematic.pdf")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "schematic_pdf" "Schematic PDF export failed")
  fi

  # Export SVG
  svg_dir="$output_dir/svg"
  mkdir -p "$svg_dir"
  run_pcb_svg_top "$pcb_file" "$svg_dir/pcb_top.svg"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "pcb_top_svg" "review/pcb_top.svg" "image/svg+xml" "$svg_dir/pcb_top.svg")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "pcb_top_svg" "PCB top SVG export failed")
  fi

  run_pcb_svg_bottom "$pcb_file" "$svg_dir/pcb_bottom.svg"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "pcb_bottom_svg" "review/pcb_bottom.svg" "image/svg+xml" "$svg_dir/pcb_bottom.svg")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "pcb_bottom_svg" "PCB bottom SVG export failed")
  fi

  # Export Gerber
  gerber_dir="$output_dir/gerber"
  mkdir -p "$gerber_dir"
  run_gerber_export "$pcb_file" "$gerber_dir"
  gerber_exit=$?

  # Export Drill
  drill_dir="$output_dir/drill"
  mkdir -p "$drill_dir"
  run_drill_export "$pcb_file" "$drill_dir"
  drill_exit=$?

  # Create zip archives for gerber/drill/fabrication before tracking artifacts
  gerbers_zip="$output_dir/gerbers.zip"
  drill_zip="$output_dir/drill.zip"
  fab_zip="$output_dir/fabrication.zip"
  if [ $gerber_exit -eq 0 ]; then
    (cd "$gerber_dir" && zip -qr "$gerbers_zip" . 2>/dev/null)
    artifacts_status=$(add_artifact_available "$artifacts_status" "gerber_zip" "fabrication/gerbers.zip" "application/zip" "$gerbers_zip")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "gerber_zip" "Gerber export failed")
  fi

  if [ $drill_exit -eq 0 ]; then
    (cd "$drill_dir" && zip -qr "$drill_zip" . 2>/dev/null)
    artifacts_status=$(add_artifact_available "$artifacts_status" "drill_zip" "fabrication/drill.zip" "application/zip" "$drill_zip")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "drill_zip" "Drill export failed")
  fi

  # Create combined fabrication.zip
  create_fabrication_zip "$gerber_dir" "$drill_dir" "$fab_zip"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "fabrication_zip" "fabrication/fabrication.zip" "application/zip" "$fab_zip")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "fabrication_zip" "Fabrication zip creation failed")
  fi

  # Export BOM
  bom_dir="$output_dir/bom"
  mkdir -p "$bom_dir"
  run_bom_export "$sch_file" "$bom_dir/bom.csv"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "bom_csv" "assembly/bom.csv" "text/csv" "$bom_dir/bom.csv")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "bom_csv" "BOM export failed")
  fi

  # Export Position
  pos_dir="$output_dir/position"
  mkdir -p "$pos_dir"
  run_position_export "$pcb_file" "$pos_dir/position.csv"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "position_csv" "assembly/position.csv" "text/csv" "$pos_dir/position.csv")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "position_csv" "Position export failed")
  fi

  # Export 3D renders
  render_dir="$output_dir/3d"
  mkdir -p "$render_dir"
  run_3d_render "$pcb_file" "$render_dir/top.png" "top"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "render_top_png" "review/render_top.png" "image/png" "$render_dir/top.png")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "render_top_png" "3D top render failed")
  fi

  run_3d_render "$pcb_file" "$render_dir/bottom.png" "bottom"
  if [ $? -eq 0 ]; then
    artifacts_status=$(add_artifact_available "$artifacts_status" "render_bottom_png" "review/render_bottom.png" "image/png" "$render_dir/bottom.png")
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "render_bottom_png" "3D bottom render failed")
  fi

  # Run iBOM
  ibom_dir="$output_dir/ibom"
  mkdir -p "$ibom_dir"
  run_ibom "$pcb_file" "$ibom_dir"
  if [ $? -eq 0 ]; then
    ibom_file=$(find "$ibom_dir" -name "*.html" -type f | head -1)
    if [ -n "$ibom_file" ]; then
      artifacts_status=$(add_artifact_available "$artifacts_status" "ibom" "assembly/ibom.html" "text/html" "$ibom_file")
    else
      artifacts_status=$(add_artifact_failed "$artifacts_status" "ibom" "iBOM HTML not found")
    fi
  else
    artifacts_status=$(add_artifact_failed "$artifacts_status" "ibom" "iBOM generation failed")
  fi

  # R-1: Add KiCad source artifacts (with source_path per spec §8.6)
  while IFS= read -r src_file; do
    [ -z "$src_file" ] && continue
    src_rel="${src_file#$project_dir/}"
    if ! is_excluded "$src_rel" "$all_excludes"; then
      kicad_type=""
      case "$src_rel" in
        *.kicad_pro) kicad_type="kicad_project" ;;
        *.kicad_sch) kicad_type="kicad_schematic" ;;
        *.kicad_pcb) kicad_type="kicad_pcb" ;;
        *.kicad_wks) kicad_type="kicad_worksheet" ;;
      esac
      staging_path="kicad/$rel_dir/$src_rel"
      source_path="$rel_dir/$src_rel"
      artifacts_status=$(add_artifact_source "$artifacts_status" "$kicad_type" "$staging_path" "$source_path" "$src_file")
    fi
  done < <(find "$project_dir" -maxdepth 1 -type f \( -name "*.kicad_pro" -o -name "*.kicad_sch" -o -name "*.kicad_pcb" -o -name "*.kicad_wks" \) | LC_ALL=C sort)

  # M-3: Generate diff metadata files
  diff_dir="$output_dir/diff"
  mkdir -p "$diff_dir"
  generate_file_hashes_json "$project_dir" "$all_excludes" "$diff_dir/file_hashes.json"
  generate_bom_summary_json "$bom_dir/bom.csv" "$diff_dir/bom_summary.json"
  generate_checks_summary_json "$erc_json" "$drc_json" "$diff_dir/checks_summary.json"
  generate_artifacts_summary_json "$artifacts_status" "$diff_dir/artifacts_summary.json"
  generate_previews_json "$output_dir" "$diff_dir/previews.json"

  # M-4: Create manifest (spec compliant)
  yml_rel="$rel_dir/.boardflow.yml"
  manifest_path="$output_dir/manifest.json"
  create_manifest "$board_project_id" "$rel_project_path" "$rel_dir" "$yml_rel" \
    "$tree_hash" "$SHA" "$REF" "$BRANCH" "$RUN_ID" "$RUN_ATTEMPT" \
    "$diff_dir/checks_summary.json" "$artifacts_status" "$diff_dir" "$manifest_path"

  # H-6: Build staging directory with spec-compliant structure
  staging_dir="$output_dir/staging"
  mkdir -p "$staging_dir/review" "$staging_dir/assembly" "$staging_dir/fabrication" "$staging_dir/checks" "$staging_dir/diff"

  # review/
  cp "$pdf_dir/schematic.pdf" "$staging_dir/review/" 2>/dev/null
  cp "$pdf_dir/pcb.pdf" "$staging_dir/review/" 2>/dev/null
  cp "$svg_dir/pcb_top.svg" "$staging_dir/review/" 2>/dev/null
  cp "$svg_dir/pcb_bottom.svg" "$staging_dir/review/" 2>/dev/null
  cp "$render_dir/top.png" "$staging_dir/review/render_top.png" 2>/dev/null
  cp "$render_dir/bottom.png" "$staging_dir/review/render_bottom.png" 2>/dev/null

  # assembly/
  cp "$ibom_dir"/*.html "$staging_dir/assembly/ibom.html" 2>/dev/null
  cp "$bom_dir/bom.csv" "$staging_dir/assembly/" 2>/dev/null
  cp "$pos_dir/position.csv" "$staging_dir/assembly/" 2>/dev/null

  # fabrication/ (H-7: individual zips)
  cp "$gerbers_zip" "$staging_dir/fabrication/gerbers.zip" 2>/dev/null
  cp "$drill_zip" "$staging_dir/fabrication/drill.zip" 2>/dev/null
  cp "$fab_zip" "$staging_dir/fabrication/fabrication.zip" 2>/dev/null

  # checks/
  cp "$erc_json" "$staging_dir/checks/erc.json" 2>/dev/null
  cp "$drc_json" "$staging_dir/checks/drc.json" 2>/dev/null

  # diff/
  cp "$diff_dir/file_hashes.json" "$staging_dir/diff/" 2>/dev/null
  cp "$diff_dir/bom_summary.json" "$staging_dir/diff/" 2>/dev/null
  cp "$diff_dir/checks_summary.json" "$staging_dir/diff/" 2>/dev/null
  cp "$diff_dir/artifacts_summary.json" "$staging_dir/diff/" 2>/dev/null
  cp "$diff_dir/previews.json" "$staging_dir/diff/" 2>/dev/null

  # H-5: Collect KiCad source files
  kicad_staging="$staging_dir/kicad/$rel_dir"
  mkdir -p "$kicad_staging"
  find "$project_dir" -maxdepth 1 \( -name "*.kicad_pro" -o -name "*.kicad_sch" -o -name "*.kicad_pcb" -o -name "*.kicad_wks" \) -type f | while IFS= read -r src_file; do
    src_rel="${src_file#$project_dir/}"
    if ! is_excluded "$src_rel" "$all_excludes"; then
      cp "$src_file" "$kicad_staging/"
    fi
  done

  # manifest.json at root
  cp "$manifest_path" "$staging_dir/manifest.json"

  bundle_path="$output_dir/bundle.zip"
  create_bundle_zip "$staging_dir" "$bundle_path"
  if [ $? -ne 0 ]; then
    echo "::error::Failed to create bundle for $rel_project_path" >&2
    call_fail_api "$board_run_id" "Bundle creation failed" "Failed to create bundle.zip"
    RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "error" \
      '. + [{"path": $path, "status": $status, "error": "bundle creation failed"}]')
    EXIT_CODE=1
    continue
  fi

  bundle_sha256=$(compute_bundle_sha256 "$bundle_path")

  # M-5: Upload bundle with timeouts
  http_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PUT \
    -H "Content-Type: application/zip" \
    --connect-timeout 30 \
    --max-time 600 \
    --data-binary "@$bundle_path" \
    "$upload_url")

  if [ "$http_status" -lt 200 ] || [ "$http_status" -ge 300 ]; then
    echo "::error::Upload failed for $rel_project_path (HTTP $http_status)" >&2
    call_fail_api "$board_run_id" "Upload failed" "HTTP status: $http_status"
    RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "error" \
      '. + [{"path": $path, "status": $status, "error": "upload failed"}]')
    EXIT_CODE=1
    continue
  fi

  # C-2: Import API payload with staging_object_key and bundle_size_bytes
  bundle_size=$(stat -c%s "$bundle_path")
  import_payload=$(jq -n \
    --arg key "$staging_object_key" \
    --arg sha256 "sha256:$bundle_sha256" \
    --argjson size "$bundle_size" \
    '{staging_object_key: $key, bundle_sha256: $sha256, bundle_size_bytes: $size}')

  call_import_api "$board_run_id" "$import_payload"
  if [ $? -ne 0 ]; then
    echo "::error::Import API failed for $rel_project_path" >&2
    call_fail_api "$board_run_id" "Import failed" "Import API call failed"
    RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "error" \
      '. + [{"path": $path, "status": $status, "error": "import failed"}]')
    EXIT_CODE=1
    continue
  fi

  RESULTS=$(echo "$RESULTS" | jq --arg path "$rel_project_path" --arg status "success" \
    '. + [{"path": $path, "status": $status}]')
done

# Write Job Summary
write_job_summary "$RESULTS"

# Set output
echo "result=$RESULTS" >> "$GITHUB_OUTPUT" 2>/dev/null || true

# H-4: Fail if detection errors occurred
if [ $DETECTION_ERRORS -gt 0 ]; then
  EXIT_CODE=1
fi

exit $EXIT_CODE
