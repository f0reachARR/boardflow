CREATE TABLE github_api_cache (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cache_type TEXT NOT NULL,
    value_json JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, cache_type)
);

CREATE INDEX idx_github_api_cache_expires_at ON github_api_cache (expires_at);

CREATE INDEX idx_users_github_access_token ON users (github_access_token) WHERE github_access_token IS NOT NULL;
