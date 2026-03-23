-- UseZombie clean-state canonical schema (UUID-only IDs)

-- Domain schemas: app data is segmented by bounded context.
CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS agent;
CREATE SCHEMA IF NOT EXISTS billing;

-- Audit schema: migration bookkeeping + immutable operator audit
-- Tables (schema_migrations, schema_migration_failures) are created by the
-- migration runner in pool.zig before any SQL files execute.
CREATE SCHEMA IF NOT EXISTS audit;

CREATE TABLE core.tenants (
    tenant_id    UUID PRIMARY KEY,
    name         TEXT NOT NULL,
    api_key_hash TEXT NOT NULL,
    created_at   BIGINT NOT NULL
);

CREATE TABLE core.workspaces (
    workspace_id    UUID PRIMARY KEY,
    tenant_id       UUID NOT NULL REFERENCES core.tenants(tenant_id),
    repo_url        TEXT NOT NULL,
    default_branch  TEXT NOT NULL,
    paused          BOOLEAN NOT NULL DEFAULT FALSE,
    paused_reason   TEXT,
    version         BIGINT NOT NULL DEFAULT 1,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);
CREATE INDEX idx_workspaces_tenant ON core.workspaces(tenant_id);

CREATE TABLE core.specs (
    spec_id      UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    tenant_id    UUID NOT NULL REFERENCES core.tenants(tenant_id),
    file_path    TEXT NOT NULL,
    title        TEXT NOT NULL,
    status       TEXT NOT NULL,
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL,
    UNIQUE (workspace_id, file_path)
);
CREATE INDEX idx_specs_workspace ON core.specs(workspace_id, status);

CREATE TABLE core.runs (
    run_id                UUID PRIMARY KEY,
    workspace_id          UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    spec_id               UUID NOT NULL REFERENCES core.specs(spec_id),
    tenant_id             UUID NOT NULL REFERENCES core.tenants(tenant_id),
    state                 TEXT NOT NULL,
    attempt               INT  NOT NULL DEFAULT 1,
    mode                  TEXT NOT NULL,
    requested_by          TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL,
    request_id            TEXT,
    trace_id              TEXT,
    branch                TEXT,
    pr_url                TEXT,
    run_snapshot_config_version  UUID,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    UNIQUE (workspace_id, idempotency_key),
    CONSTRAINT ck_runs_run_id_uuidv7 CHECK (substring(run_id::text from 15 for 1) = '7'),
    CONSTRAINT ck_runs_snapshot_config_uuidv7 CHECK (run_snapshot_config_version IS NULL OR substring(run_snapshot_config_version::text from 15 for 1) = '7')
);
CREATE INDEX idx_runs_state ON core.runs(state, created_at);
CREATE INDEX idx_runs_workspace ON core.runs(workspace_id, state);
CREATE INDEX idx_runs_request_id ON core.runs(request_id);
CREATE INDEX idx_runs_trace_id ON core.runs(trace_id);
CREATE INDEX idx_runs_snapshot_config_version ON core.runs(run_snapshot_config_version, created_at DESC);

CREATE TABLE core.run_transitions (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_run_transitions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id       UUID NOT NULL REFERENCES core.runs(run_id),
    attempt      INT  NOT NULL,
    state_from   TEXT NOT NULL,
    state_to     TEXT NOT NULL,
    actor        TEXT NOT NULL,
    reason_code  TEXT NOT NULL,
    notes        TEXT,
    ts           BIGINT NOT NULL
);
CREATE INDEX idx_transitions_run ON core.run_transitions(run_id, ts ASC) INCLUDE (state_from, state_to, actor, reason_code);

CREATE TABLE core.artifacts (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_artifacts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id           UUID NOT NULL REFERENCES core.runs(run_id),
    attempt          INT  NOT NULL,
    artifact_name    TEXT NOT NULL,
    object_key       TEXT NOT NULL,
    checksum_sha256  TEXT NOT NULL,
    producer         TEXT NOT NULL,
    created_at       BIGINT NOT NULL,
    UNIQUE (run_id, attempt, artifact_name)
);
CREATE INDEX idx_artifacts_run ON core.artifacts(run_id, attempt DESC, artifact_name);

CREATE TABLE billing.usage_ledger (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_usage_ledger_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id        UUID NOT NULL REFERENCES core.runs(run_id),
    workspace_id  UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    attempt       INT  NOT NULL,
    actor         TEXT NOT NULL,
    event_key     TEXT,
    lifecycle_event TEXT NOT NULL,
    billable_unit TEXT NOT NULL,
    billable_quantity BIGINT NOT NULL DEFAULT 0,
    is_billable   BOOLEAN NOT NULL DEFAULT FALSE,
    source        TEXT NOT NULL,
    token_count   BIGINT NOT NULL DEFAULT 0,
    agent_seconds BIGINT NOT NULL DEFAULT 0,
    created_at    BIGINT NOT NULL
);
CREATE UNIQUE INDEX idx_usage_ledger_run_event_key
    ON billing.usage_ledger (run_id, event_key)
    WHERE event_key IS NOT NULL;
CREATE INDEX idx_usage_ledger_workspace
    ON billing.usage_ledger (workspace_id, created_at DESC);
CREATE INDEX idx_usage_ledger_billable
    ON billing.usage_ledger (run_id, attempt, is_billable, billable_unit);
CREATE INDEX idx_usage_run ON billing.usage_ledger(run_id, attempt, source);

CREATE TABLE core.workspace_memories (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_workspace_memories_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    run_id       UUID NOT NULL REFERENCES core.runs(run_id),
    content      TEXT NOT NULL,
    tags         TEXT NOT NULL DEFAULT '[]',
    created_at   BIGINT NOT NULL,
    expires_at   BIGINT
);
CREATE INDEX idx_memories_workspace ON core.workspace_memories(workspace_id, created_at DESC);

CREATE TABLE core.policy_events (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_policy_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id       UUID REFERENCES core.runs(run_id),
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    action_class TEXT NOT NULL,
    decision     TEXT NOT NULL,
    rule_id      TEXT NOT NULL,
    actor        TEXT NOT NULL,
    ts           BIGINT NOT NULL
);
CREATE INDEX idx_policy_workspace ON core.policy_events(workspace_id, ts DESC) INCLUDE (action_class, decision);
