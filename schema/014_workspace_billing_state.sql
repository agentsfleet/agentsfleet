-- M6_001: Paid Scale plan lifecycle and deterministic billing-state sync

ALTER TABLE entitlement_policy_audit_snapshots
    DROP CONSTRAINT IF EXISTS entitlement_policy_audit_snapshots_boundary_check;

ALTER TABLE entitlement_policy_audit_snapshots
    ADD CONSTRAINT entitlement_policy_audit_snapshots_boundary_check
    CHECK (boundary IN ('COMPILE', 'ACTIVATE', 'RUNTIME'));

CREATE TABLE IF NOT EXISTS workspace_billing_state (
    billing_id         UUID PRIMARY KEY,
    workspace_id       UUID NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    plan_tier          TEXT NOT NULL CHECK (plan_tier IN ('FREE', 'SCALE')),
    plan_sku           TEXT NOT NULL,
    billing_status     TEXT NOT NULL CHECK (billing_status IN ('ACTIVE', 'GRACE', 'DOWNGRADED')),
    adapter            TEXT NOT NULL DEFAULT 'noop',
    subscription_id    TEXT,
    payment_failed_at  BIGINT,
    grace_expires_at   BIGINT,
    pending_status     TEXT CHECK (pending_status IN ('ACTIVATE_SCALE', 'PAYMENT_FAILED', 'DOWNGRADE_TO_FREE')),
    pending_reason     TEXT,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_billing_state_plan
    ON workspace_billing_state (plan_tier, billing_status, updated_at DESC);

CREATE TABLE IF NOT EXISTS workspace_billing_audit (
    audit_id            UUID PRIMARY KEY,
    workspace_id        UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
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
    ON workspace_billing_audit (workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON workspace_billing_state TO api_accessor;
GRANT SELECT, INSERT, UPDATE, DELETE ON workspace_billing_audit TO api_accessor;
GRANT SELECT ON workspace_billing_state TO worker_accessor;
GRANT SELECT ON workspace_billing_audit TO worker_accessor;
