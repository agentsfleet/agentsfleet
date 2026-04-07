CREATE TABLE IF NOT EXISTS IF NOT EXISTS billing.workspace_credit_state (
    credit_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL UNIQUE REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    currency TEXT NOT NULL,
    initial_credit_cents BIGINT NOT NULL CHECK (initial_credit_cents >= 0),
    consumed_credit_cents BIGINT NOT NULL CHECK (consumed_credit_cents >= 0),
    remaining_credit_cents BIGINT NOT NULL CHECK (remaining_credit_cents >= 0),
    exhausted_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS IF NOT EXISTS idx_workspace_credit_state_remaining
    ON billing.workspace_credit_state (remaining_credit_cents, updated_at DESC);

CREATE TABLE IF NOT EXISTS IF NOT EXISTS billing.workspace_credit_audit (
    audit_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    delta_credit_cents BIGINT NOT NULL,
    remaining_credit_cents BIGINT NOT NULL CHECK (remaining_credit_cents >= 0),
    reason TEXT NOT NULL,
    actor TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    created_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS IF NOT EXISTS idx_workspace_credit_audit_workspace
    ON billing.workspace_credit_audit (workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.workspace_credit_state TO api_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON billing.workspace_credit_audit TO api_runtime;
GRANT SELECT ON billing.workspace_credit_state TO worker_runtime;
GRANT SELECT ON billing.workspace_credit_audit TO worker_runtime;
