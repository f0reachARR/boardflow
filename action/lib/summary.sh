#!/bin/bash
# summary.sh - GitHub Actions Job Summary output

# Write Job Summary with project results
write_job_summary() {
  local results="$1"
  local summary_file="${GITHUB_STEP_SUMMARY:-/dev/null}"

  {
    echo "## BoardFlow Action Results"
    echo ""
    echo "| Project | Status | Details |"
    echo "|---------|--------|---------|"

    echo "$results" | jq -r '.[] | "| \(.path) | \(.status) | \(.error // "-") |"'

    echo ""

    local total
    total=$(echo "$results" | jq 'length')
    local success
    success=$(echo "$results" | jq '[.[] | select(.status == "success")] | length')
    local skipped
    skipped=$(echo "$results" | jq '[.[] | select(.status == "skipped")] | length')
    local errors
    errors=$(echo "$results" | jq '[.[] | select(.status == "error")] | length')

    echo "**Total:** $total projects | **Success:** $success | **Skipped:** $skipped | **Errors:** $errors"
  } >> "$summary_file"
}

# Write summary for unsupported events
write_unsupported_event_summary() {
  local event_name="$1"
  local summary_file="${GITHUB_STEP_SUMMARY:-/dev/null}"

  {
    echo "## BoardFlow Action"
    echo ""
    echo "⚠️ Unsupported event: \`$event_name\`"
    echo ""
    echo "BoardFlow Action only runs on push events. Pull request support is planned for a future release."
  } >> "$summary_file"
}
