-- UseZombie M1 schema
-- All tables use append-only patterns where noted.
-- Run: psql $DATABASE_URL < schema/001_initial.sql

-- ── Tenants ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS tenants (
    tenant_id    TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    api_key_hash TEXT NOT NULL,
    created_at   BIGINT NOT NULL  -- Unix ms
);

-- ── Workspaces ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS workspaces (
    workspace_id    TEXT PRIMARY KEY,
    tenant_id       TEXT NOT NULL REFERENCES tenants(tenant_id),
    repo_url        TEXT NOT NULL,
    default_branch  TEXT NOT NULL DEFAULT 'main',
    paused          BOOLEAN NOT NULL DEFAULT FALSE,
    paused_reason   TEXT,
    version         BIGINT NOT NULL DEFAULT 1,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant ON workspaces(tenant_id);

-- ── Specs ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS specs (
    spec_id      TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
    tenant_id    TEXT NOT NULL,
    file_path    TEXT NOT NULL,  -- relative path e.g. docs/spec/PENDING_001.md
    title        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',  -- pending|in_progress|done|failed
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL,
    UNIQUE (workspace_id, file_path)
);
CREATE INDEX IF NOT EXISTS idx_specs_workspace ON specs(workspace_id, status);

-- ── Runs ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS runs (
    run_id          TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id),
    spec_id         TEXT NOT NULL REFERENCES specs(spec_id),
    tenant_id       TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'SPEC_QUEUED',
    attempt         INT  NOT NULL DEFAULT 1,
    mode            TEXT NOT NULL DEFAULT 'api',  -- web|api
    requested_by    TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_id      TEXT,
    branch          TEXT,
    pr_url          TEXT,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    UNIQUE (workspace_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_runs_state      ON runs(state, created_at);
CREATE INDEX IF NOT EXISTS idx_runs_workspace  ON runs(workspace_id, state);
CREATE INDEX IF NOT EXISTS idx_runs_request_id ON runs(request_id);

-- ── Run transitions (append-only audit trail) ─────────────────────────────
CREATE TABLE IF NOT EXISTS run_transitions (
    id           BIGSERIAL PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES runs(run_id),
    attempt      INT  NOT NULL,
    state_from   TEXT NOT NULL,
    state_to     TEXT NOT NULL,
    actor        TEXT NOT NULL,  -- echo|scout|warden|orchestrator
    reason_code  TEXT NOT NULL,
    notes        TEXT,
    ts           BIGINT NOT NULL  -- Unix ms
);
CREATE INDEX IF NOT EXISTS idx_transitions_run ON run_transitions(run_id, ts);

-- ── Artifacts ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS artifacts (
    id               BIGSERIAL PRIMARY KEY,
    run_id           TEXT    NOT NULL REFERENCES runs(run_id),
    attempt          INT     NOT NULL,
    artifact_name    TEXT    NOT NULL,  -- plan.json, implementation.md, etc.
    object_key       TEXT    NOT NULL,  -- git path: docs/runs/<run_id>/<name>
    checksum_sha256  TEXT    NOT NULL,
    producer         TEXT    NOT NULL,  -- echo|scout|warden|orchestrator
    created_at       BIGINT  NOT NULL,
    UNIQUE (run_id, attempt, artifact_name)
);

-- ── Usage ledger ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS usage_ledger (
    id            BIGSERIAL PRIMARY KEY,
    run_id        TEXT   NOT NULL REFERENCES runs(run_id),
    attempt       INT    NOT NULL,
    actor         TEXT   NOT NULL,
    token_count   BIGINT NOT NULL DEFAULT 0,
    agent_seconds BIGINT NOT NULL DEFAULT 0,
    created_at    BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_run ON usage_ledger(run_id);

-- ── Workspace memories (cross-run context) ────────────────────────────────
CREATE TABLE IF NOT EXISTS workspace_memories (
    id           BIGSERIAL PRIMARY KEY,
    workspace_id TEXT   NOT NULL REFERENCES workspaces(workspace_id),
    run_id       TEXT   NOT NULL,
    content      TEXT   NOT NULL,
    tags         TEXT   NOT NULL DEFAULT '[]',  -- JSON array
    created_at   BIGINT NOT NULL,
    expires_at   BIGINT           -- NULL = never expires
);
CREATE INDEX IF NOT EXISTS idx_memories_workspace ON workspace_memories(workspace_id, created_at DESC);

-- ── Policy events ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS policy_events (
    id           BIGSERIAL PRIMARY KEY,
    run_id       TEXT,
    workspace_id TEXT   NOT NULL,
    action_class TEXT   NOT NULL,  -- safe|sensitive|critical
    decision     TEXT   NOT NULL,  -- allow|deny|require_confirmation
    rule_id      TEXT   NOT NULL,
    actor        TEXT   NOT NULL,
    ts           BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_workspace ON policy_events(workspace_id, ts DESC);

-- ── Secrets (AES-256-GCM encrypted at rest) ───────────────────────────────
CREATE TABLE IF NOT EXISTS secrets (
    id           BIGSERIAL PRIMARY KEY,
    workspace_id TEXT   NOT NULL REFERENCES workspaces(workspace_id),
    key_name     TEXT   NOT NULL,
    ciphertext   TEXT   NOT NULL,  -- base64url(nonce||ct||tag)
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL,
    UNIQUE (workspace_id, key_name)
);
