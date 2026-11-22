-- Immutable interpretation receipts; they outlive their source turn, so `turn_id` is opaque.
CREATE TABLE fleet.durable_task_items (
    id                  UUID PRIMARY KEY,
    fleet_id            UUID NOT NULL,
    workspace_id        UUID NOT NULL,
    turn_id             UUID NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    plan_digest         TEXT NOT NULL,
    action              TEXT NOT NULL,
    source_start        INTEGER NOT NULL CHECK (source_start >= 0),
    source_end          INTEGER NOT NULL CHECK (source_end >= source_start),
    durable_task_id             UUID,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_items_fleet FOREIGN KEY (fleet_id)
        REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_items_source_ordinal UNIQUE (turn_id, ordinal),
    CONSTRAINT uq_durable_task_items_id_scope UNIQUE (id, fleet_id, workspace_id)
);

CREATE INDEX idx_durable_task_items_fleet_id ON fleet.durable_task_items (fleet_id);

CREATE INDEX idx_durable_task_items_durable_task_scope ON fleet.durable_task_items (durable_task_id, fleet_id, workspace_id)
    WHERE durable_task_id IS NOT NULL;

CREATE FUNCTION fleet.durable_task_items_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'durable_task_items is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_items_refuse_update
    BEFORE UPDATE ON fleet.durable_task_items
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_items_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_items TO api_runtime;
