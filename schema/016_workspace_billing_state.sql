-- M6_001: Paid Scale plan lifecycle and deterministic billing-state sync

CREATE TABLE IF NOT EXISTS billing.workspace_billing_state (
    billing_id         UUID PRIMARY KEY,
    workspace_id       UUID NOT NULL UNIQUE REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    plan_tier          TEXT NOT NULL,
    plan_sku           TEXT NOT NULL,
    billing_status     TEXT NOT NULL,
    adapter            TEXT NOT NULL,
    subscription_id    TEXT,
    payment_failed_at  BIGINT,
    grace_expires_at   BIGINT,
    pending_status     TEXT,
    pending_reason     TEXT,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_billing_state_plan
    ON billing.workspace_billing_state (plan_tier, billing_status, updated_at DESC);

CREATE TABLE IF NOT EXISTS billing.workspace_billing_audit (
    audit_id            UUID PRIMARY KEY,
    workspace_id        UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    event_type          TEXT NOT NULL,
    previous_plan_tier  TEXT,
    new_plan_tier       TEXT NOT NULL,
    previous_status     TEXT,
    new_status          TEXT NOT NULL,
    reason              TEXT NOT NULL,
    actor               TEXT NOT NULL,
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_billing_audit_workspace
    ON billing.workspace_billing_audit (workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.workspace_billing_state TO api_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON billing.workspace_billing_audit TO api_runtime;
GRANT SELECT ON billing.workspace_billing_state TO worker_runtime;
GRANT SELECT ON billing.workspace_billing_audit TO worker_runtime;
