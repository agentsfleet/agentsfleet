-- 003_core_results_events.sql
-- Core results and events: gate_results (append-only), billing.usage_ledger,
-- workspace_memories, and policy_events.
-- Split from the original monolithic 001_initial.sql.

CREATE TABLE core.gate_results (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_gate_results_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id           UUID NOT NULL REFERENCES core.runs(run_id) ON DELETE CASCADE,
    gate_name        TEXT NOT NULL,
    attempt          INT  NOT NULL,
    exit_code        INT  NOT NULL,
    stdout_tail      TEXT,
    stderr_tail      TEXT,
    wall_ms          BIGINT NOT NULL,
    created_at       BIGINT NOT NULL
);
CREATE INDEX idx_gate_results_run ON core.gate_results(run_id, gate_name, attempt);

-- gate_results is append-only: block UPDATE/DELETE via trigger.
CREATE OR REPLACE FUNCTION core.gate_results_append_only() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'gate_results is append-only — UPDATE and DELETE are not permitted';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_gate_results_append_only
    BEFORE UPDATE OR DELETE ON core.gate_results
    FOR EACH ROW EXECUTE FUNCTION core.gate_results_append_only();

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
    updated_at   BIGINT NOT NULL,
    expires_at   BIGINT
);
CREATE INDEX idx_memories_workspace ON core.workspace_memories(workspace_id, created_at DESC);

-- M21_001: interrupt event log (not a state transition — dedicated table)
CREATE TABLE core.run_interrupts (
    id           UUID PRIMARY KEY,
    CONSTRAINT ck_run_interrupts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    run_id       UUID NOT NULL REFERENCES core.runs(run_id),
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    agent_id     UUID,
    attempt      INT  NOT NULL,
    mode         TEXT NOT NULL,
    message      TEXT NOT NULL,
    delivered    BOOLEAN NOT NULL DEFAULT FALSE,
    actor        TEXT NOT NULL,
    created_at   BIGINT NOT NULL
);
CREATE INDEX idx_run_interrupts_run ON core.run_interrupts(run_id, created_at DESC);
CREATE INDEX idx_run_interrupts_workspace ON core.run_interrupts(workspace_id, created_at DESC);

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
