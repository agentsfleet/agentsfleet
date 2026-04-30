-- Approval gate audit log for Zombie actions.
-- Records every gate decision (approve, deny, timeout, auto_kill) for audit.
-- Append-only by design: DELETE blocked, UPDATE allowed only on pending rows.
-- The pending-row UPDATE precondition IS the dedup mechanism for resolve --
-- Slack callback and dashboard handler race against the same WHERE clause;
-- first writer wins, second observes RETURNING 0 rows and surfaces 409.

CREATE TABLE IF NOT EXISTS core.zombie_approval_gates (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombie_approval_gates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    action_id       TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    action_name     TEXT NOT NULL,

    -- Inbox-visible fields surfaced to operators in the dashboard.
    -- gate_kind: classification driving UI grouping/filtering ('destructive_action' | 'cost_overrun' | 'external_call' | '').
    -- proposed_action / blast_radius: human-readable prose for the detail page.
    -- evidence: agent-gathered context (free-form JSON; rendered as expandable tree).
    -- timeout_at: epoch ms; sweeper transitions pending → timed_out at or after this point.
    -- No DEFAULT — every INSERT supplies timeout_at via recordGatePending, and a
    -- DEFAULT of 0 would sweep every pre-existing pending row on the next cycle
    -- (0 ≤ now_ms is always true), auto-denying gates outside the writer's intent.
    -- resolved_by: attribution of the resolver across channels
    --   ('user:<email>' | 'slack:<user_id>' | 'api:<key_id>' | 'system:timeout').
    gate_kind       TEXT NOT NULL DEFAULT '',
    proposed_action TEXT NOT NULL DEFAULT '',
    evidence        JSONB NOT NULL DEFAULT '{}'::jsonb,
    blast_radius    TEXT NOT NULL DEFAULT '',
    timeout_at      BIGINT NOT NULL,
    resolved_by     TEXT NOT NULL DEFAULT '',

    -- status: enum maintained in application code (src/zombie/approval_gate.zig
    -- GateStatus). No DEFAULT here — every INSERT supplies it explicitly via
    -- approval_gate.GateStatus.<variant>.toSlice() so a rename of an enum
    -- variant cannot drift past the type system.
    status          TEXT NOT NULL,
    detail          TEXT NOT NULL DEFAULT '',
    requested_at    BIGINT NOT NULL,
    updated_at      BIGINT,
    created_at      BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_zombie_status
    ON core.zombie_approval_gates (zombie_id, status);

CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_action_id
    ON core.zombie_approval_gates (action_id);

-- Inbox list query: oldest pending rows per workspace.
CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_workspace_status_requested
    ON core.zombie_approval_gates (workspace_id, status, requested_at);

-- Sweeper query: pending rows past their timeout_at, scanned every cycle.
CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_pending_timeout
    ON core.zombie_approval_gates (timeout_at)
    WHERE status = 'pending';

CREATE OR REPLACE FUNCTION core.zombie_approval_gates_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'zombie_approval_gates is append-only -- DELETE is not permitted';
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.status != 'pending' THEN
        RAISE EXCEPTION 'zombie_approval_gates -- only pending rows can be updated';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_zombie_approval_gates_append_only
    BEFORE UPDATE OR DELETE ON core.zombie_approval_gates
    FOR EACH ROW EXECUTE FUNCTION core.zombie_approval_gates_append_only();

GRANT SELECT, INSERT, UPDATE ON core.zombie_approval_gates TO worker_runtime;
GRANT SELECT, INSERT, UPDATE ON core.zombie_approval_gates TO api_runtime;
