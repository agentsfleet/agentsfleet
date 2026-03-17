-- M5_004: Usage metering and billing adapter integration
-- Extends immutable usage_ledger rows with deterministic event keys and
-- billable metadata, and adds idempotent delivery outbox for billing adapters.
-- usage_ledger canonical columns and indexes now live in schema/001_initial.sql.

CREATE TABLE IF NOT EXISTS billing_delivery_outbox (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_billing_delivery_outbox_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id            UUID NOT NULL REFERENCES runs(run_id),
    workspace_id      UUID NOT NULL REFERENCES workspaces(workspace_id),
    attempt           INT NOT NULL,
    idempotency_key   TEXT NOT NULL UNIQUE,
    billable_unit     TEXT NOT NULL,
    billable_quantity BIGINT NOT NULL CHECK (billable_quantity >= 0),
    status            TEXT NOT NULL,
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
