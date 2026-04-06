-- 002_core_workflow.sql
-- Core workflow: specs, runs, run_transitions, and artifacts.
-- Split from the original monolithic 001_initial.sql.

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
    base_commit_sha       TEXT,
    run_snapshot_config_version  UUID,
    dedup_key             TEXT,
    -- M17_001 §1.1: per-run enforcement limits (immutable once enqueued)
    max_repair_loops      INT NOT NULL DEFAULT 3,
    max_tokens            BIGINT NOT NULL DEFAULT 100000,
    max_wall_time_seconds BIGINT NOT NULL DEFAULT 600,
    tokens_used           BIGINT NOT NULL DEFAULT 0,
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
CREATE UNIQUE INDEX idx_runs_dedup_key ON core.runs(dedup_key) WHERE dedup_key IS NOT NULL;

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
