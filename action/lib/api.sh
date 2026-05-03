#!/bin/bash
# api.sh - SaaS API calls with retry logic

# Generic API request with retry (3 attempts, exponential backoff for 5xx/timeout)
api_request() {
  local method="$1"
  local endpoint="$2"
  local body="${3:-}"

  local url="${API_URL}${endpoint}"
  local max_retries=3
  local attempt=0
  local backoff=1

  while [ $attempt -lt $max_retries ]; do
    attempt=$((attempt + 1))
    local response
    local curl_args=(
      -s
      -X "$method"
      -H "Authorization: Bearer $TOKEN"
      -H "Content-Type: application/json"
      -w "\n%{http_code}"
      --connect-timeout 30
      --max-time 60
    )

    if [ -n "$body" ]; then
      curl_args+=(-d "$body")
    fi

    response=$(curl "${curl_args[@]}" "$url" 2>/dev/null)
    local curl_exit=$?

    if [ $curl_exit -ne 0 ]; then
      # Connection timeout or error
      if [ $attempt -lt $max_retries ]; then
        echo "Retry $attempt: curl error $curl_exit for $endpoint" >&2
        sleep $backoff
        backoff=$((backoff * 2))
        continue
      fi
      echo "API request failed after $max_retries attempts: $endpoint" >&2
      return 1
    fi

    local http_status
    http_status=$(echo "$response" | tail -n1)
    local response_body
    response_body=$(echo "$response" | sed '$d')

    if [ "$http_status" -ge 500 ] 2>/dev/null; then
      if [ $attempt -lt $max_retries ]; then
        echo "Retry $attempt: HTTP $http_status for $endpoint" >&2
        sleep $backoff
        backoff=$((backoff * 2))
        continue
      fi
      echo "API request failed with HTTP $http_status after $max_retries attempts: $endpoint" >&2
      return 1
    fi

    if [ "$http_status" -ge 400 ] 2>/dev/null; then
      echo "API error HTTP $http_status: $response_body" >&2
      return 1
    fi

    echo "$response_body"
    return 0
  done

  return 1
}

# POST /api/v1/runs/plan (H-1: return .projects not .decisions)
call_plan_api() {
  local payload_json="$1"
  local response
  response=$(api_request "POST" "/api/v1/runs/plan" "$payload_json")
  if [ $? -ne 0 ]; then
    return 1
  fi
  echo "$response" | jq '.projects'
}

# POST /api/v1/board-runs
call_create_board_run() {
  local payload_json="$1"
  api_request "POST" "/api/v1/board-runs" "$payload_json"
}

# POST /api/v1/board-runs/{id}/artifact-bundles/import
call_import_api() {
  local board_run_id="$1"
  local payload_json="$2"
  api_request "POST" "/api/v1/board-runs/${board_run_id}/artifact-bundles/import" "$payload_json"
}

# POST /api/v1/board-runs/{id}/fail (H-3: spec-compliant payload)
call_fail_api() {
  local board_run_id="$1"
  local message="$2"
  local details="${3:-}"
  local payload
  payload=$(jq -n --arg msg "$message" --arg det "$details" \
    '{status: "failed", error: {message: $msg, details: $det}}')
  api_request "POST" "/api/v1/board-runs/${board_run_id}/fail" "$payload"
}
