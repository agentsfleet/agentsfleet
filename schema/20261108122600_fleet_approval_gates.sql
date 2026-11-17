-- A risky action waiting for a person; it outlives the run that raised it. `resolved_at` is NULL
-- while pending and set once. No DELETE grant: a gate goes only with its fleet's purge cascade.
CREATE TABLE fleet.approval_gates (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_approval_gates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
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
    stated_binding    JSONB,
    blast_radius      TEXT   NOT NULL,
    spend_count       BIGINT,
    spend_ceiling     BIGINT,
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
    IF OLD.resolved_at IS NULL THEN
        IF NEW.spend_count IS DISTINCT FROM OLD.spend_count
            OR NEW.spend_ceiling IS DISTINCT FROM OLD.spend_ceiling THEN
            RAISE EXCEPTION 'approval_gates spend is fixed before resolution';
        END IF;
        RETURN NEW;
    END IF;
    -- Resolved: the only change allowed is spending one more use within the ceiling.
    IF (NEW.id, NEW.fleet_id, NEW.workspace_id, NEW.event_id, NEW.active_grant_id,
        NEW.action_id, NEW.tool_name, NEW.action_name, NEW.gate_type,
        NEW.proposed_action, NEW.evidence, NEW.stated_binding, NEW.blast_radius,
        NEW.spend_ceiling, NEW.expires_at, NEW.status, NEW.resolved_at,
        NEW.resolved_by, NEW.resolved_by_name, NEW.detail, NEW.created_at,
        NEW.updated_at)
        IS DISTINCT FROM
       (OLD.id, OLD.fleet_id, OLD.workspace_id, OLD.event_id, OLD.active_grant_id,
        OLD.action_id, OLD.tool_name, OLD.action_name, OLD.gate_type,
        OLD.proposed_action, OLD.evidence, OLD.stated_binding, OLD.blast_radius,
        OLD.spend_ceiling, OLD.expires_at, OLD.status, OLD.resolved_at,
        OLD.resolved_by, OLD.resolved_by_name, OLD.detail, OLD.created_at,
        OLD.updated_at)
        OR OLD.spend_count IS NULL
        OR OLD.spend_ceiling IS NULL
        OR NEW.spend_count IS DISTINCT FROM OLD.spend_count + 1
        OR NEW.spend_count > NEW.spend_ceiling THEN
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
