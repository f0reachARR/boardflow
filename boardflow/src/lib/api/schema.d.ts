/**
 * OpenAPI schema type definitions for BoardFlow API.
 * Generated manually based on docs/backend/api.md.
 * Will be replaced by openapi-typescript generation once backend serves OpenAPI spec.
 */

export interface paths {
  "/api/v1/auth/me": {
    get: {
      responses: {
        200: {
          content: {
            "application/json": {
              user_id: string
              github_login: string
              github_avatar_url: string | null
            }
          }
        }
        401: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/auth/logout": {
    post: {
      responses: {
        200: {
          content: {
            "application/json": { ok: true }
          }
        }
      }
    }
  }
  "/api/v1/repositories": {
    get: {
      parameters: {
        query?: {
          limit?: number
          cursor?: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": PaginatedResponse<Repository>
          }
        }
      }
    }
  }
  "/api/v1/repositories/{github_repository_id}": {
    get: {
      parameters: {
        path: {
          github_repository_id: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": RepositoryDetail
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/repositories/{github_repository_id}/board-projects": {
    get: {
      parameters: {
        path: {
          github_repository_id: string
        }
        query?: {
          limit?: number
          cursor?: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": PaginatedResponse<BoardProjectSummary>
          }
        }
      }
    }
  }
  "/api/v1/board-projects/{board_project_id}": {
    get: {
      parameters: {
        path: {
          board_project_id: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": BoardProjectDetail
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/board-projects/{board_project_id}/board-runs": {
    get: {
      parameters: {
        path: {
          board_project_id: string
        }
        query?: {
          limit?: number
          cursor?: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": PaginatedResponse<BoardRunSummary>
          }
        }
      }
    }
  }
  "/api/v1/board-runs/{board_run_id}": {
    get: {
      parameters: {
        path: {
          board_run_id: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": BoardRunDetail
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/board-runs/{board_run_id}/artifacts": {
    get: {
      parameters: {
        path: {
          board_run_id: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": { items: Artifact[] }
          }
        }
      }
    }
  }
  "/api/v1/board-runs/{board_run_id}/viewer-sources": {
    get: {
      parameters: {
        path: {
          board_run_id: string
        }
      }
      responses: {
        200: {
          content: {
            "application/json": ViewerSourcesResponse
          }
        }
      }
    }
  }
  "/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings": {
    get: {
      parameters: {
        path: { board_run_id: string; check_kind: "erc" | "drc" }
        query?: { limit?: number; cursor?: string; severity?: "error" | "warning" | "notice" }
      }
      responses: {
        200: {
          content: {
            "application/json": PaginatedResponse<Finding>
          }
        }
        400: {
          content: {
            "application/json": ApiError
          }
        }
        401: {
          content: {
            "application/json": ApiError
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/board-runs/{board_run_id}/diff": {
    get: {
      parameters: {
        path: { board_run_id: string }
      }
      responses: {
        200: {
          content: {
            "application/json": DiffResponse
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/repositories/{github_repository_id}/api-tokens": {
    get: {
      parameters: {
        path: { github_repository_id: string }
        query?: { limit?: number; cursor?: string }
      }
      responses: {
        200: {
          content: {
            "application/json": PaginatedResponse<ApiToken>
          }
        }
        401: {
          content: {
            "application/json": ApiError
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
    post: {
      parameters: { path: { github_repository_id: string } }
      requestBody: { content: { "application/json": { name: string } } }
      responses: {
        201: {
          content: {
            "application/json": ApiTokenCreated
          }
        }
        400: {
          content: {
            "application/json": ApiError
          }
        }
        401: {
          content: {
            "application/json": ApiError
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
  "/api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke": {
    post: {
      parameters: {
        path: { github_repository_id: string; token_id: string }
      }
      responses: {
        200: {
          content: {
            "application/json": ApiToken
          }
        }
        400: {
          content: {
            "application/json": ApiError
          }
        }
        401: {
          content: {
            "application/json": ApiError
          }
        }
        404: {
          content: {
            "application/json": ApiError
          }
        }
      }
    }
  }
}

// Common types

export interface ApiError {
  error: {
    code: string
    message: string
    details?: Record<string, unknown>
    request_id?: string
  }
}

export interface PaginatedResponse<T> {
  items: T[]
  next_cursor: string | null
  has_more: boolean
}

// Domain types

export interface Repository {
  github_repository_id: string
  owner: string
  name: string
  installation_id: string
  board_project_count: number
  latest_run_status: string | null
  updated_at: string
}

export interface RepositoryDetail {
  github_repository_id: string
  owner: string
  name: string
  installation_id: string
  html_url: string
  board_project_count: number
  created_at: string
  updated_at: string
}

export interface BoardProjectSummary {
  board_project_id: string
  project_path: string
  project_dir: string
  display_name: string
  state: string
  latest_completed_run_id: string | null
  latest_tree_hash: string | null
  issue_url: string | null
  updated_at: string
}

export interface BoardProjectDetail {
  board_project_id: string
  repository: {
    github_repository_id: string
    owner: string
    name: string
  }
  project_path: string
  project_dir: string
  display_name: string
  state: string
  latest_completed_run_id: string | null
  latest_tree_hash: string | null
  issue_number: number | null
  issue_url: string | null
  recreate_issue_on_update: boolean
  created_at: string
  updated_at: string
}

export interface BoardRunSummary {
  board_run_id: string
  status: string
  commit_sha: string
  branch: string
  ref: string
  github_run_id: string
  github_run_attempt: string
  tree_hash: string
  erc_status: string | null
  erc_errors: number | null
  erc_warnings: number | null
  drc_status: string | null
  drc_errors: number | null
  drc_warnings: number | null
  created_at: string
  completed_at: string | null
}

export interface BoardRunDetail {
  board_run_id: string
  board_project_id: string
  status: string
  commit_sha: string
  branch: string
  ref: string
  github_run_id: string
  github_run_attempt: string
  tree_hash: string
  checks: Check[]
  artifact_summary: {
    available: number
    missing: number
    failed: number
    skipped: number
  }
  created_at: string
  completed_at: string | null
}

export interface Check {
  kind: string
  status: string
  error_count: number
  warning_count: number
  notice_count: number
}

export interface Artifact {
  artifact_id?: string
  type: string
  status: "available" | "missing" | "failed" | "skipped"
  filename?: string
  content_type?: string
  sha256?: string
  size_bytes?: number
  source_path?: string | null
  logical_name?: string | null
  status_reason?: string
  created_at?: string
}

export interface ViewerSourcesResponse {
  board_run_id: string
  expires_at: string
  viewers: Record<string, ViewerEntry>
}

export interface ViewerEntry {
  status: "available" | "partial" | "missing" | "failed" | "skipped"
  sources?: ViewerSource[]
  primary?: ViewerSource
  iframe_url?: string
  downloads?: ViewerDownload[]
}

export interface ViewerSource {
  artifact_id: string
  kind?: string
  artifact_type?: string
  name?: string
  source_path?: string
  url: string
}

export interface ViewerDownload {
  artifact_id?: string
  artifact_type: string
  status?: string
  url?: string
  status_reason?: string
}

export interface Finding {
  id: string
  severity: "error" | "warning" | "notice"
  rule_code: string
  title: string
  message: string | null
  subject_kind: string | null
  subject_ref: string | null
  sheet_path: string | null
  pcb_layer: string | null
  pos_mm: { x: number; y: number } | null
}

export interface DiffResponse {
  board_run_id: string
  base_board_run_id: string | null
  status: "ready" | "no_baseline" | "unavailable" | "failed"
  summary: DiffSummary | null
  metadata: Record<string, unknown> | null
  error_message: string | null
  created_at: string
}

export interface DiffSummary {
  file_changes: { added: number; removed: number; changed: number; unchanged: number }
  bom_changes: { added: number; removed: number; changed: number }
  checks: Record<string, { status_change: string; error_delta: number; warning_delta: number }>
  artifacts: { added: number; removed: number; changed: number }
}

export interface ApiToken {
  id: string
  name: string
  created_at: string
  last_used_at: string | null
  revoked_at: string | null
}

export interface ApiTokenCreated {
  id: string
  name: string
  token: string
  created_at: string
}
