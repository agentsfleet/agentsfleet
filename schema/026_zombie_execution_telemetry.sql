-- M18_001: Per-delivery execution telemetry store.
-- One row per zombie event delivery, keyed on event_id (UNIQUE — idempotent on replay).
-- Non-billing: credit audit remains in workspace_credit_audit; this table is for
-- latency and token observability accessible via customer and operator APIs.

CREATE TABLE zombie_execution_telemetry (
    id                       TEXT        NOT NULL PRIMARY KEY,
    zombie_id                TEXT        NOT NULL,
    workspace_id             TEXT        NOT NULL,
    event_id                 TEXT        NOT NULL,
    token_count              BIGINT      NOT NULL DEFAULT 0,
    time_to_first_token_ms   BIGINT      NOT NULL DEFAULT 0,
    epoch_wall_time_ms       BIGINT      NOT NULL DEFAULT 0,
    wall_seconds             BIGINT      NOT NULL DEFAULT 0,
    plan_tier                TEXT        NOT NULL DEFAULT 'free',
    credit_deducted_cents    BIGINT      NOT NULL DEFAULT 0,
    recorded_at              BIGINT      NOT NULL,
    CONSTRAINT uq_telemetry_event_id UNIQUE (event_id)
);

-- Customer query: workspace + zombie, newest-first (cursor pagination).
CREATE INDEX idx_telemetry_workspace_zombie
    ON zombie_execution_telemetry (workspace_id, zombie_id, recorded_at DESC);

-- Operator query: workspace filter + time-window.
CREATE INDEX idx_telemetry_workspace_time
    ON zombie_execution_telemetry (workspace_id, recorded_at DESC);

-- Operator query: zombie_id-only filter (workspace_id is optional in listTelemetryAll).
CREATE INDEX idx_telemetry_zombie
    ON zombie_execution_telemetry (zombie_id, recorded_at DESC);
