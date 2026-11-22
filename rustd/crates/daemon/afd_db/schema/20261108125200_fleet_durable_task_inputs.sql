-- Accepted input meaning, kept apart from pending queue delivery. `sequence` is per-task order,
-- allocated under the task row's lock. Rows are written once, so UPDATE is refused.
CREATE TABLE fleet.durable_task_inputs (
    id                  UUID PRIMARY KEY,
    durable_task_id             UUID NOT NULL,
    fleet_id            UUID NOT NULL,
    workspace_id        UUID NOT NULL,
    durable_task_item_id        UUID NOT NULL,
    sequence            BIGINT NOT NULL CHECK (sequence > 0),
    accepted_text       TEXT NOT NULL,
    interpretation      TEXT NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_inputs_durable_task_scope FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_durable_task_inputs_item_scope FOREIGN KEY (durable_task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_task_items (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_inputs_durable_task_item UNIQUE (durable_task_id, durable_task_item_id),
    CONSTRAINT uq_durable_task_inputs_durable_task_sequence UNIQUE (durable_task_id, sequence)
);

CREATE INDEX idx_durable_task_inputs_item_scope ON fleet.durable_task_inputs (durable_task_item_id, fleet_id, workspace_id);

CREATE FUNCTION fleet.durable_task_inputs_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'durable_task_inputs is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_inputs_refuse_update
    BEFORE UPDATE ON fleet.durable_task_inputs
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_inputs_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_inputs TO api_runtime;
