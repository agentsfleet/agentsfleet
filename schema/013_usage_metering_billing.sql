-- M5_004: Usage metering and billing adapter integration
-- Extends immutable usage_ledger rows with deterministic event keys and
-- billable metadata, and adds idempotent delivery outbox for billing adapters.

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS workspace_id UUID;

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS event_key TEXT;

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS lifecycle_event TEXT NOT NULL DEFAULT 'stage_completed';

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS billable_unit TEXT NOT NULL DEFAULT 'agent_second';

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS billable_quantity BIGINT NOT NULL DEFAULT 0;

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS is_billable BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'runtime_stage';

UPDATE usage_ledger ul
SET workspace_id = r.workspace_id
FROM runs r
WHERE ul.workspace_id IS NULL
  AND r.run_id = ul.run_id;

ALTER TABLE usage_ledger
    ALTER COLUMN workspace_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_usage_ledger_workspace'
    ) THEN
        ALTER TABLE usage_ledger
            ADD CONSTRAINT fk_usage_ledger_workspace
            FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id);
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_ledger_run_event_key
    ON usage_ledger (run_id, event_key)
    WHERE event_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_usage_ledger_workspace
    ON usage_ledger (workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_ledger_billable
    ON usage_ledger (run_id, attempt, is_billable, billable_unit);

CREATE TABLE IF NOT EXISTS billing_delivery_outbox (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_billing_delivery_outbox_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id            UUID NOT NULL REFERENCES runs(run_id),
    workspace_id      UUID NOT NULL REFERENCES workspaces(workspace_id),
    attempt           INT NOT NULL,
    idempotency_key   TEXT NOT NULL UNIQUE,
    billable_unit     TEXT NOT NULL,
    billable_quantity BIGINT NOT NULL CHECK (billable_quantity >= 0),
    status            TEXT NOT NULL DEFAULT 'pending',
    delivery_attempts INT NOT NULL DEFAULT 0,
    next_retry_at     BIGINT NOT NULL DEFAULT 0,
    adapter           TEXT NOT NULL,
    adapter_reference TEXT,
    last_error        TEXT,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL,
    delivered_at      BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_delivery_run_attempt_unit
    ON billing_delivery_outbox (run_id, attempt, billable_unit);

CREATE INDEX IF NOT EXISTS idx_billing_delivery_status_retry
    ON billing_delivery_outbox (status, next_retry_at, created_at);
