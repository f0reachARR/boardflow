#!/bin/bash
# hash.sh - File hash and tree_hash computation

# Built-in exclude patterns
BUILTIN_EXCLUDES="**/*.lck
**/*.bak
**/*-backups/**
**/fp-info-cache
**/.DS_Store
**/output/**
**/outputs/**
**/fabrication/**
**/gerber/**
**/gerbers/**"

# Check if a path matches any exclude pattern using fnmatch (C-3: use sys.argv/stdin)
is_excluded() {
  local path="$1"
  local excludes="$2"
  echo "$excludes" | python3 - "$path" <<'PYTHON'
import fnmatch, sys
path = sys.argv[1]
excludes = sys.stdin.read().strip().splitlines()
for pattern in excludes:
    pattern = pattern.strip()
    if not pattern:
        continue
    if fnmatch.fnmatch(path, pattern):
        sys.exit(0)
sys.exit(1)
PYTHON
}

# List project files filtered by excludes (M-1: batch filtering in single Python call)
list_project_files() {
  local project_dir="$1"
  local excludes="$2"

  find "$project_dir" -type f 2>/dev/null | \
    EXCLUDES="$excludes" python3 - "$project_dir" <<'PYTHON'
import fnmatch, sys, os
project_dir = sys.argv[1]
excludes = os.environ.get('EXCLUDES', '').strip().splitlines()
excludes = [p.strip() for p in excludes if p.strip()]
for line in sys.stdin:
    filepath = line.strip()
    if not filepath:
        continue
    rel = os.path.relpath(filepath, project_dir)
    excluded = False
    for pattern in excludes:
        if fnmatch.fnmatch(rel, pattern):
            excluded = True
            break
    if not excluded:
        print(rel)
PYTHON
}

# Compute SHA256 of a file
compute_file_sha256() {
  local path="$1"
  sha256sum "$path" | cut -d' ' -f1
}

# Compute tree hash: sha256(sorted(relative_path\0sha256\n))
compute_tree_hash() {
  local project_dir="$1"
  local excludes="$2"

  local entries=""
  while IFS= read -r rel_path; do
    [ -z "$rel_path" ] && continue
    local file_hash
    file_hash=$(compute_file_sha256 "$project_dir/$rel_path")
    entries+="${rel_path}\x00${file_hash}\n"
  done < <(list_project_files "$project_dir" "$excludes" | LC_ALL=C sort)

  printf "%b" "$entries" | sha256sum | cut -d' ' -f1
}
