-- UseZombie clean-state canonical schema (UUID-only IDs)

CREATE TABLE tenants (
    tenant_id    UUID PRIMARY KEY,
    name         TEXT NOT NULL,
    api_key_hash TEXT NOT NULL,
    created_at   BIGINT NOT NULL
);

CREATE TABLE workspaces (
    workspace_id    UUID PRIMARY KEY,
    tenant_id       UUID NOT NULL REFERENCES tenants(tenant_id),
    repo_url        TEXT NOT NULL,
    default_branch  TEXT NOT NULL DEFAULT 'main',
    paused          BOOLEAN NOT NULL DEFAULT FALSE,
    paused_reason   TEXT,
    version         BIGINT NOT NULL DEFAULT 1,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);
CREATE INDEX idx_workspaces_tenant ON workspaces(tenant_id);

CREATE TABLE specs (
    spec_id      UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(workspace_id),
    tenant_id    UUID NOT NULL REFERENCES tenants(tenant_id),
    file_path    TEXT NOT NULL,
    title        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL,
    UNIQUE (workspace_id, file_path)
);
CREATE INDEX idx_specs_workspace ON specs(workspace_id, status);

CREATE TABLE runs (
    run_id                UUID PRIMARY KEY,
    workspace_id          UUID NOT NULL REFERENCES workspaces(workspace_id),
    spec_id               UUID NOT NULL REFERENCES specs(spec_id),
    tenant_id             UUID NOT NULL REFERENCES tenants(tenant_id),
    state                 TEXT NOT NULL DEFAULT 'SPEC_QUEUED',
    attempt               INT  NOT NULL DEFAULT 1,
    mode                  TEXT NOT NULL DEFAULT 'api',
    requested_by          TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL,
    request_id            TEXT,
    trace_id              TEXT,
    branch                TEXT,
    pr_url                TEXT,
    run_snapshot_version  UUID,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    UNIQUE (workspace_id, idempotency_key),
    CONSTRAINT ck_runs_run_id_uuidv7 CHECK (substring(run_id::text from 15 for 1) = '7'),
    CONSTRAINT ck_runs_snapshot_uuidv7 CHECK (run_snapshot_version IS NULL OR substring(run_snapshot_version::text from 15 for 1) = '7')
);
CREATE INDEX idx_runs_state ON runs(state, created_at);
CREATE INDEX idx_runs_workspace ON runs(workspace_id, state);
CREATE INDEX idx_runs_request_id ON runs(request_id);
CREATE INDEX idx_runs_trace_id ON runs(trace_id);
CREATE INDEX idx_runs_snapshot_version ON runs(run_snapshot_version, created_at DESC);

CREATE TABLE run_transitions (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_run_transitions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id       UUID NOT NULL REFERENCES runs(run_id),
    attempt      INT  NOT NULL,
    state_from   TEXT NOT NULL,
    state_to     TEXT NOT NULL,
    actor        TEXT NOT NULL,
    reason_code  TEXT NOT NULL,
    notes        TEXT,
    ts           BIGINT NOT NULL
);
CREATE INDEX idx_transitions_run ON run_transitions(run_id, ts ASC) INCLUDE (state_from, state_to, actor, reason_code);

CREATE TABLE artifacts (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_artifacts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id           UUID NOT NULL REFERENCES runs(run_id),
    attempt          INT  NOT NULL,
    artifact_name    TEXT NOT NULL,
    object_key       TEXT NOT NULL,
    checksum_sha256  TEXT NOT NULL,
    producer         TEXT NOT NULL,
    created_at       BIGINT NOT NULL,
    UNIQUE (run_id, attempt, artifact_name)
);
CREATE INDEX idx_artifacts_run ON artifacts(run_id, attempt DESC, artifact_name);

CREATE TABLE usage_ledger (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_usage_ledger_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id        UUID NOT NULL REFERENCES runs(run_id),
    attempt       INT  NOT NULL,
    actor         TEXT NOT NULL,
    token_count   BIGINT NOT NULL DEFAULT 0,
    agent_seconds BIGINT NOT NULL DEFAULT 0,
    created_at    BIGINT NOT NULL
);
CREATE INDEX idx_usage_run ON usage_ledger(run_id, attempt, source);

CREATE TABLE workspace_memories (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_workspace_memories_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL REFERENCES workspaces(workspace_id),
    run_id       UUID NOT NULL REFERENCES runs(run_id),
    content      TEXT NOT NULL,
    tags         TEXT NOT NULL DEFAULT '[]',
    created_at   BIGINT NOT NULL,
    expires_at   BIGINT
);
CREATE INDEX idx_memories_workspace ON workspace_memories(workspace_id, created_at DESC);

CREATE TABLE policy_events (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_policy_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id       UUID REFERENCES runs(run_id),
    workspace_id UUID NOT NULL REFERENCES workspaces(workspace_id),
    action_class TEXT NOT NULL,
    decision     TEXT NOT NULL,
    rule_id      TEXT NOT NULL,
    actor        TEXT NOT NULL,
    ts           BIGINT NOT NULL
);
CREATE INDEX idx_policy_workspace ON policy_events(workspace_id, ts DESC) INCLUDE (action_class, decision);

