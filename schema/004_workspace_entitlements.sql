-- Entitlement source-of-truth (UUID-only IDs)

CREATE TABLE IF NOT EXISTS billing.workspace_entitlements (
    entitlement_id       UUID PRIMARY KEY,
    workspace_id         UUID NOT NULL UNIQUE REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    plan_tier            TEXT NOT NULL,
    max_stages           INTEGER NOT NULL CHECK (max_stages > 0),
    max_distinct_skills  INTEGER NOT NULL CHECK (max_distinct_skills > 0),
    allow_custom_skills  BOOLEAN NOT NULL,
    enable_agent_scoring BOOLEAN NOT NULL,
    agent_scoring_weights_json TEXT NOT NULL,
    scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    CONSTRAINT ck_workspace_entitlements_scoring_context_max_tokens
        CHECK (scoring_context_max_tokens >= 512 AND scoring_context_max_tokens <= 8192)
);
CREATE INDEX IF NOT EXISTS idx_workspace_entitlements_tier
    ON billing.workspace_entitlements(plan_tier, updated_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.workspace_entitlements TO api_runtime;
GRANT SELECT ON billing.workspace_entitlements TO worker_runtime;
