-- Approval-gate audit log for fleet actions: every gate decision — approve,
-- deny, timeout, auto_kill — recorded for audit.
--
-- Append-only by trigger: DELETE is refused, and UPDATE is permitted only while
-- the row is pending. That precondition IS the dedup mechanism for resolution —
-- the connector callback and the dashboard handler race against the same WHERE
-- clause, the first writer wins, and the second sees zero rows returned and
-- surfaces a conflict.
--
-- `requested_at` is gone, folded into `created_at`. Both columns existed and both
-- writers bound the same parameter to each — the gate row is created by the
-- request, so they were one instant under two names. The inbox
-- keyset orders by `(created_at, id)` accordingly.
--
--   gate_kind        classification driving inbox grouping and filtering
--   proposed_action  human-readable prose for the detail page
--   blast_radius
--   evidence         fleet-gathered context, rendered as an expandable tree
--   timeout_at       the sweeper transitions pending → timed_out at or after
--                    this instant. NO DEFAULT, deliberately: a default of zero
--                    would sweep every pending row on the next cycle,
--                    auto-denying gates outside the writer's intent.
--   resolved_by      attribution across channels
--   status           vocabulary in fleet/approval_gate.zig GateStatus. No
--                    DEFAULT — every INSERT supplies it explicitly, so renaming
--                    a variant cannot drift past the type system (RULE STS).

CREATE TABLE IF NOT EXISTS core.fleet_approval_gates (
    id              UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_approval_gates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id        UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    workspace_id    UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    action_id       TEXT   NOT NULL,
    tool_name       TEXT   NOT NULL,
    action_name     TEXT   NOT NULL,
    gate_kind       TEXT   NOT NULL,
    proposed_action TEXT   NOT NULL,
    evidence        JSONB  NOT NULL,
    blast_radius    TEXT   NOT NULL,
    timeout_at      BIGINT NOT NULL,
    resolved_by     TEXT   NOT NULL,
    status          TEXT   NOT NULL,
    detail          TEXT   NOT NULL,
    created_at      BIGINT NOT NULL,
    -- Nullable, unlike every other mutable table here: a still-pending gate has
    -- genuinely never been updated, and its one update is its resolution.
    updated_at      BIGINT
);

-- Reader: the per-fleet gate lookup, filtered by fleet and lifecycle state.
CREATE INDEX IF NOT EXISTS idx_fleet_approval_gates_fleet_id_status
    ON core.fleet_approval_gates (fleet_id, status);

-- Reader: the connector callback, which resolves an inbound approval to its gate
-- by the action identifier it carries.
CREATE INDEX IF NOT EXISTS idx_fleet_approval_gates_action_id
    ON core.fleet_approval_gates (action_id);

-- Reader: the operator inbox — a workspace's pending gates, oldest first.
CREATE INDEX IF NOT EXISTS idx_fleet_approval_gates_workspace_id_status_created_at
    ON core.fleet_approval_gates (workspace_id, status, created_at);

-- Reader: the timeout sweeper, which scans pending rows past their deadline
-- every cycle. The literal in the predicate names its application constant: it
-- is fleet/approval_gate.zig GateStatus.pending, and a partial index requires a
-- SQL predicate to express it. The same value appears in the
-- trigger below, and the two must agree.
CREATE INDEX IF NOT EXISTS idx_fleet_approval_gates_timeout_at_pending
    ON core.fleet_approval_gates (timeout_at)
    WHERE status = 'pending';

-- Append-only enforcement, plus the one deliberate exception. Both hard-purge
-- paths — a personal account erasure and a fleet hard-delete — must remove gate
-- rows, and now reach them by cascade from the foreign keys above rather than by
-- an explicit delete statement. The cascade still fires this
-- trigger, so those transactions opt in with a transaction-scoped setting that
-- dies with the transaction; every other DELETE still raises.
--
-- The `pending` literal mirrors GateStatus.pending, as in the sweeper index above.
CREATE OR REPLACE FUNCTION core.fleet_approval_gates_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF current_setting('fleet.allow_gate_purge', true) = 'on' THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'fleet_approval_gates is append-only -- DELETE is not permitted';
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.status != 'pending' THEN
        RAISE EXCEPTION 'fleet_approval_gates -- only pending rows can be updated';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_fleet_approval_gates_append_only
    BEFORE UPDATE OR DELETE ON core.fleet_approval_gates
    FOR EACH ROW EXECUTE FUNCTION core.fleet_approval_gates_append_only();

-- No DELETE grant: removal is the cascade's, gated by the setting above.
GRANT SELECT, INSERT, UPDATE ON core.fleet_approval_gates TO api_runtime;
