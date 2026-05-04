#!/bin/bash
# config.sh - .boardflow.yml schema validation and exclude_paths parsing

# Parse .boardflow.yml to JSON using python3 (C-3: use sys.argv for path)
parse_boardflow_yml() {
  local path="$1"
  python3 - "$path" <<'PYTHON'
import yaml, json, sys
try:
    with open(sys.argv[1], 'r') as f:
        data = yaml.safe_load(f)
    if data is None:
        data = {}
    print(json.dumps(data))
except Exception as e:
    print(str(e), file=sys.stderr)
    sys.exit(1)
PYTHON
}

# Validate schema version 1
validate_schema_v1() {
  local json="$1"
  local valid
  valid=$(echo "$json" | python3 -c "
import json, sys

data = json.load(sys.stdin)
allowed_top = {'version', 'outputs', 'exclude_paths'}
allowed_outputs = {'preset'}

# Check version
if data.get('version') != 1:
    print('invalid version', file=sys.stderr)
    sys.exit(1)

# Check unknown top-level fields
unknown = set(data.keys()) - allowed_top
if unknown:
    print(f'unknown fields: {unknown}', file=sys.stderr)
    sys.exit(1)

# Check outputs fields if present
if 'outputs' in data and isinstance(data['outputs'], dict):
    unknown_out = set(data['outputs'].keys()) - allowed_outputs
    if unknown_out:
        print(f'unknown output fields: {unknown_out}', file=sys.stderr)
        sys.exit(1)

print('ok')
" 2>&1)

  if [ "$valid" = "ok" ]; then
    return 0
  else
    echo "$valid" >&2
    return 1
  fi
}

# Extract exclude_paths from config JSON
get_exclude_paths_from_config() {
  local json="$1"
  echo "$json" | jq -r '.exclude_paths // [] | .[]' 2>/dev/null
}

# Merge three exclude lists into union (newline-separated)
merge_excludes() {
  local builtin="$1"
  local input="$2"
  local yml="$3"

  {
    echo "$builtin"
    echo "$input"
    echo "$yml"
  } | grep -v '^$' | sort -u
}
