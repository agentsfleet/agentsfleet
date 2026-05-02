-- Per-event execution telemetry. Two rows per event under the credit-pool
-- billing model (charge_type ∈ {receive, stage}); UNIQUE (event_id, charge_type).
-- The receive row is INSERTed at gate-pass; the stage row is INSERTed before
-- startStage and UPDATEd post-execution with token counts and wall_ms.
--
-- Value constraints on `charge_type` and `posture` are enforced in application
-- code via constants in src/state/tenant_provider.zig and
-- src/state/zombie_telemetry_store.zig — RULE STS forbids static-string CHECKs.

CREATE TABLE zombie_execution_telemetry (
    id                       TEXT   NOT NULL PRIMARY KEY,
    tenant_id                UUID   NOT NULL,
    workspace_id             TEXT   NOT NULL,
    zombie_id                TEXT   NOT NULL,
    event_id                 TEXT   NOT NULL,
    charge_type              TEXT   NOT NULL,
    posture                  TEXT   NOT NULL,
    model                    TEXT   NOT NULL,
    credit_deducted_cents    BIGINT NOT NULL DEFAULT 0,
    token_count_input        BIGINT NULL,
    token_count_output       BIGINT NULL,
    wall_ms                  BIGINT NULL,
    recorded_at              BIGINT NOT NULL,
    CONSTRAINT uq_telemetry_event_charge UNIQUE (event_id, charge_type)
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

-- Tenant-scoped Usage tab query: GET /v1/tenants/me/billing/usage.
CREATE INDEX idx_telemetry_tenant_time
    ON zombie_execution_telemetry (tenant_id, recorded_at DESC);

-- api_runtime: customer + operator + tenant Usage read endpoints (SELECT),
-- metering INSERT from HTTP path.
GRANT SELECT, INSERT, UPDATE ON zombie_execution_telemetry TO api_runtime;
-- worker_runtime: event-loop metering writes (receive INSERT pre-stage,
-- stage INSERT pre-execution, stage UPDATE post-execution).
GRANT INSERT, UPDATE ON zombie_execution_telemetry TO worker_runtime;
