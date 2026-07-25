ALTER TABLE core.workspaces
    ADD COLUMN IF NOT EXISTS create_idempotency_key UUID,
    ADD COLUMN IF NOT EXISTS create_request_name TEXT,
    ADD COLUMN IF NOT EXISTS create_request_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_workspaces_tenant_create_idempotency
    ON core.workspaces(tenant_id, create_idempotency_key)
    WHERE create_idempotency_key IS NOT NULL;
