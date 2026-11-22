-- Accepted input meaning, kept apart from pending queue delivery. `sequence` is per-task order,
-- allocated under the task row's lock. The meaning is written once, so its UPDATE is refused. An
-- input also keeps its one apply receipt, NULL together until the run that applied it writes it
-- and then written once: the retry key and request digest and the run and fence that wrote it.
-- Read by `afd_fleet/src/durable_task/input_receipt/sql/replay.sql`.
CREATE TABLE fleet.durable_task_inputs (
    id                  UUID PRIMARY KEY,
    durable_task_id             UUID NOT NULL,
    fleet_id            UUID NOT NULL,
    workspace_id        UUID NOT NULL,
    durable_task_item_id        UUID NOT NULL,
    sequence            BIGINT NOT NULL CHECK (sequence > 0),
    accepted_text       TEXT NOT NULL,
    interpretation      TEXT NOT NULL,
    operation_id        UUID,
    run_id              UUID,
    fencing_token       BIGINT,
    request_digest      TEXT,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_inputs_durable_task_scope FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_durable_task_inputs_item_scope FOREIGN KEY (durable_task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_task_items (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_inputs_durable_task_item UNIQUE (durable_task_id, durable_task_item_id),
    CONSTRAINT uq_durable_task_inputs_durable_task_sequence UNIQUE (durable_task_id, sequence),
    CONSTRAINT ck_durable_task_inputs_receipt_all_or_none CHECK (
        num_nulls(operation_id, run_id, fencing_token, request_digest) IN (0, 4))
);

CREATE INDEX idx_durable_task_inputs_item_scope ON fleet.durable_task_inputs (durable_task_item_id, fleet_id, workspace_id);

-- One operation answers one receipt per fleet; an item's reply receipt is checked beside it.
CREATE UNIQUE INDEX uq_durable_task_inputs_fleet_operation ON fleet.durable_task_inputs (fleet_id, operation_id)
    WHERE operation_id IS NOT NULL;

CREATE FUNCTION fleet.durable_task_inputs_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'durable_task_inputs is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_inputs_refuse_update
    BEFORE UPDATE OF id, durable_task_id, fleet_id, workspace_id, durable_task_item_id, sequence,
        accepted_text, interpretation, created_at ON fleet.durable_task_inputs
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_inputs_refuse_update();

CREATE TRIGGER trg_durable_task_inputs_refuse_receipt
    BEFORE UPDATE OF operation_id, run_id, fencing_token, request_digest ON fleet.durable_task_inputs
    FOR EACH ROW WHEN (OLD.request_digest IS NOT NULL)
    EXECUTE FUNCTION fleet.durable_task_inputs_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_inputs TO api_runtime;
GRANT UPDATE (operation_id, run_id, fencing_token, request_digest, updated_at)
    ON fleet.durable_task_inputs TO api_runtime;
