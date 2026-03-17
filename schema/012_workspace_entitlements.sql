-- Entitlement source-of-truth and policy audit snapshots (UUID-only IDs)

CREATE TABLE workspace_entitlements (
    entitlement_id       UUID PRIMARY KEY,
    workspace_id         UUID NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    plan_tier            TEXT NOT NULL,
    max_profiles         INTEGER NOT NULL CHECK (max_profiles > 0),
    max_stages           INTEGER NOT NULL CHECK (max_stages > 0),
    max_distinct_skills  INTEGER NOT NULL CHECK (max_distinct_skills > 0),
    allow_custom_skills  BOOLEAN NOT NULL,
    enable_agent_scoring BOOLEAN NOT NULL,
    agent_scoring_weights_json TEXT NOT NULL,
    enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE,
    scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    CONSTRAINT ck_workspace_entitlements_scoring_context_max_tokens
        CHECK (scoring_context_max_tokens >= 512 AND scoring_context_max_tokens <= 8192)
);
CREATE INDEX idx_workspace_entitlements_tier
    ON workspace_entitlements(plan_tier, updated_at DESC);

CREATE TABLE entitlement_policy_audit_snapshots (
    snapshot_id        UUID PRIMARY KEY,
    workspace_id       UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    boundary           TEXT NOT NULL,
    decision           TEXT NOT NULL,
    reason_code        TEXT NOT NULL,
    plan_tier          TEXT NOT NULL,
    policy_json        TEXT NOT NULL DEFAULT '{}',
    observed_json      TEXT NOT NULL DEFAULT '{}',
    actor              TEXT NOT NULL,
    created_at         BIGINT NOT NULL
);
CREATE INDEX idx_entitlement_policy_audit_workspace
    ON entitlement_policy_audit_snapshots(workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON workspace_entitlements TO api_accessor;
GRANT SELECT ON workspace_entitlements TO worker_accessor;

GRANT SELECT, INSERT, UPDATE, DELETE ON entitlement_policy_audit_snapshots TO api_accessor;
GRANT SELECT ON entitlement_policy_audit_snapshots TO worker_accessor;
