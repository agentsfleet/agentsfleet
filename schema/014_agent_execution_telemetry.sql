-- Per-event execution telemetry. Two rows per event under the credit-pool
-- billing model (charge_type ∈ {receive, stage}); UNIQUE (event_id, charge_type).
-- The receive row is INSERTed at gate-pass; the stage row is INSERTed before
-- startStage and UPDATEd post-execution with token counts and wall_ms.
--
-- Value constraints on `charge_type` and `posture` are enforced in application
-- code via constants in src/state/tenant_provider.zig and
-- src/state/agent_telemetry_store.zig — RULE STS forbids static-string CHECKs.

CREATE TABLE core.agent_execution_telemetry (
    uid                      UUID   PRIMARY KEY,
    CONSTRAINT ck_agent_execution_telemetry_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id                       TEXT   NOT NULL UNIQUE,
    tenant_id                UUID   NOT NULL,
    workspace_id             TEXT   NOT NULL,
    agent_id                TEXT   NOT NULL,
    event_id                 TEXT   NOT NULL,
    charge_type              TEXT   NOT NULL,
    posture                  TEXT   NOT NULL,
    model                    TEXT   NOT NULL,
    credit_deducted_nanos    BIGINT NOT NULL DEFAULT 0,
    token_count_input        BIGINT NULL,
    token_count_output       BIGINT NULL,
    wall_ms                  BIGINT NULL,
    recorded_at              BIGINT NOT NULL,
    CONSTRAINT uq_telemetry_event_charge UNIQUE (event_id, charge_type)
);

-- Customer query: workspace + agent, newest-first (cursor pagination).
CREATE INDEX idx_telemetry_workspace_agent
    ON core.agent_execution_telemetry (workspace_id, agent_id, recorded_at DESC);

-- Operator query: workspace filter + time-window.
CREATE INDEX idx_telemetry_workspace_time
    ON core.agent_execution_telemetry (workspace_id, recorded_at DESC);

-- Operator query: agent_id-only filter (workspace_id is optional in listTelemetryAll).
CREATE INDEX idx_telemetry_agent
    ON core.agent_execution_telemetry (agent_id, recorded_at DESC);

-- Tenant-scoped charges query: GET /v1/tenants/me/billing/charges.
CREATE INDEX idx_telemetry_tenant_time
    ON core.agent_execution_telemetry (tenant_id, recorded_at DESC);

-- api_runtime: customer + operator + tenant Usage read endpoints (SELECT),
-- metering INSERT from the HTTP path, plus the event-loop metering writes
-- (receive INSERT pre-stage, stage INSERT pre-execution, stage UPDATE post).
GRANT SELECT, INSERT, UPDATE ON core.agent_execution_telemetry TO api_runtime;
