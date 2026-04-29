-- M4_001: Approval gate audit log for Zombie actions.
-- Records every gate decision (approve, deny, timeout, auto_kill) for audit.
-- Append-only by design. UPDATE and DELETE blocked by trigger.

CREATE TABLE IF NOT EXISTS core.zombie_approval_gates (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombie_approval_gates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    action_id       TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    action_name     TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    detail          TEXT NOT NULL DEFAULT '',
    requested_at    BIGINT NOT NULL,
    updated_at      BIGINT,
    created_at      BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_zombie_status
    ON core.zombie_approval_gates (zombie_id, status);

CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_action_id
    ON core.zombie_approval_gates (action_id);

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
