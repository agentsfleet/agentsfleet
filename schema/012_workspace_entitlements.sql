-- UseZombie M5_003 entitlement source-of-truth and policy audit snapshots.

CREATE TABLE IF NOT EXISTS workspace_entitlements (
    entitlement_id       UUID PRIMARY KEY,
    workspace_id         TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    plan_tier            TEXT NOT NULL CHECK (plan_tier IN ('FREE', 'SCALE')),
    max_profiles         INTEGER NOT NULL CHECK (max_profiles > 0),
    max_stages           INTEGER NOT NULL CHECK (max_stages > 0),
    max_distinct_skills  INTEGER NOT NULL CHECK (max_distinct_skills > 0),
    allow_custom_skills  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workspace_entitlements_tier
    ON workspace_entitlements(plan_tier, updated_at DESC);

CREATE TABLE IF NOT EXISTS entitlement_policy_audit_snapshots (
    snapshot_id        UUID PRIMARY KEY,
    workspace_id       TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    boundary           TEXT NOT NULL CHECK (boundary IN ('COMPILE', 'ACTIVATE')),
    decision           TEXT NOT NULL CHECK (decision IN ('ALLOW', 'DENY')),
    reason_code        TEXT NOT NULL,
    plan_tier          TEXT NOT NULL,
    policy_json        TEXT NOT NULL DEFAULT '{}',
    observed_json      TEXT NOT NULL DEFAULT '{}',
    actor              TEXT NOT NULL,
    created_at         BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entitlement_policy_audit_workspace
    ON entitlement_policy_audit_snapshots(workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON workspace_entitlements TO api_accessor;
GRANT SELECT ON workspace_entitlements TO worker_accessor;

GRANT SELECT, INSERT, UPDATE, DELETE ON entitlement_policy_audit_snapshots TO api_accessor;
GRANT SELECT ON entitlement_policy_audit_snapshots TO worker_accessor;
