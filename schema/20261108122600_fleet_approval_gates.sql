-- A risky action waiting for a person; it outlives the run that raised it. `resolved_at` is NULL
-- while pending and set once. No DELETE grant: a gate goes only with its fleet's purge cascade.
CREATE TABLE fleet.approval_gates (
    id                UUID PRIMARY KEY,
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id      UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    event_id          UUID,
    active_grant_id   UUID REFERENCES connector.integration_grants(id),
    action_id         UUID   NOT NULL,
    tool_name         TEXT   NOT NULL,
    action_name       TEXT   NOT NULL,
    gate_type         TEXT   NOT NULL,
    proposed_action   TEXT   NOT NULL,
    evidence          JSONB  NOT NULL,
    blast_radius      TEXT   NOT NULL,
    expires_at        TIMESTAMPTZ(3) NOT NULL,
    status            TEXT   NOT NULL,
    resolved_at       TIMESTAMPTZ(3),
    resolved_by       TEXT,
    resolved_by_name  TEXT,
    detail            TEXT   NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX uq_approval_gates_active_grant_id ON fleet.approval_gates (active_grant_id);

CREATE INDEX idx_approval_gates_action_id ON fleet.approval_gates (action_id);

CREATE INDEX idx_approval_gates_fleet_id_event_id
    ON fleet.approval_gates (fleet_id, event_id) WHERE event_id IS NOT NULL;

CREATE INDEX idx_approval_gates_fleet_id_status ON fleet.approval_gates (fleet_id, status);

CREATE INDEX idx_approval_gates_expires_at_pending
    ON fleet.approval_gates (expires_at) WHERE resolved_at IS NULL;

CREATE INDEX idx_approval_gates_workspace_id_created_at_id
    ON fleet.approval_gates (workspace_id, created_at, id);

CREATE INDEX idx_approval_gates_workspace_id_status_created_at
    ON fleet.approval_gates (workspace_id, status, created_at);

CREATE FUNCTION fleet.approval_gates_guard_update() RETURNS trigger AS $$
BEGIN
    IF OLD.resolved_at IS NOT NULL THEN
        RAISE EXCEPTION 'approval_gates resolved row is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_approval_gates_guard_update
    BEFORE UPDATE ON fleet.approval_gates
    FOR EACH ROW EXECUTE FUNCTION fleet.approval_gates_guard_update();

GRANT SELECT, INSERT, UPDATE ON fleet.approval_gates TO api_runtime;

CREATE INDEX idx_approval_gates_pending_scope_deadline
    ON fleet.approval_gates (fleet_id, event_id, expires_at)
    WHERE resolved_at IS NULL;
