-- One approved repository-write gate funds a bounded number of credential
-- requests. Existing and non-write rows remain NULL; new write rows carry the
-- application-owned zero count and fixed ceiling.

ALTER TABLE core.fleet_approval_gates
    ADD COLUMN IF NOT EXISTS spend_count BIGINT;

ALTER TABLE core.fleet_approval_gates
    ADD COLUMN IF NOT EXISTS spend_ceiling BIGINT;

CREATE OR REPLACE FUNCTION core.fleet_approval_gates_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF current_setting('fleet.allow_gate_purge', true) = 'on' THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'fleet_approval_gates is append-only -- DELETE is not permitted';
    END IF;
    IF OLD.status = 'pending' THEN
        IF NEW.spend_count IS DISTINCT FROM OLD.spend_count
            OR NEW.spend_ceiling IS DISTINCT FROM OLD.spend_ceiling
        THEN
            RAISE EXCEPTION 'fleet_approval_gates spend is fixed before resolution';
        END IF;
        RETURN NEW;
    END IF;
    IF OLD.status IS DISTINCT FROM 'approved'
        OR NEW.status IS DISTINCT FROM OLD.status
        OR NEW.id IS DISTINCT FROM OLD.id
        OR NEW.fleet_id IS DISTINCT FROM OLD.fleet_id
        OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
        OR NEW.action_id IS DISTINCT FROM OLD.action_id
        OR NEW.tool_name IS DISTINCT FROM OLD.tool_name
        OR NEW.action_name IS DISTINCT FROM OLD.action_name
        OR NEW.gate_kind IS DISTINCT FROM OLD.gate_kind
        OR NEW.proposed_action IS DISTINCT FROM OLD.proposed_action
        OR NEW.evidence IS DISTINCT FROM OLD.evidence
        OR NEW.blast_radius IS DISTINCT FROM OLD.blast_radius
        OR NEW.timeout_at IS DISTINCT FROM OLD.timeout_at
        OR NEW.resolved_by IS DISTINCT FROM OLD.resolved_by
        OR NEW.detail IS DISTINCT FROM OLD.detail
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
        OR NEW.updated_at IS DISTINCT FROM OLD.updated_at
        OR NEW.event_id IS DISTINCT FROM OLD.event_id
        OR NEW.stated_binding IS DISTINCT FROM OLD.stated_binding
        OR NEW.spend_ceiling IS DISTINCT FROM OLD.spend_ceiling
        OR OLD.spend_count IS NULL
        OR OLD.spend_ceiling IS NULL
        OR NEW.spend_count IS DISTINCT FROM OLD.spend_count + 1
        OR NEW.spend_count > NEW.spend_ceiling
    THEN
        RAISE EXCEPTION 'fleet_approval_gates terminal row is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
