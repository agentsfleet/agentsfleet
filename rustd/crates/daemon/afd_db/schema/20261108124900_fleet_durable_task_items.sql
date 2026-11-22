-- Immutable interpretation receipts; they outlive their source turn, so `turn_id` is opaque. An
-- item also keeps its one reply receipt, NULL together until written and then written once:
-- `queued` or `answered` (`mode`), the retry key and request digest, the run and fence that
-- wrote it, and the private reply. Read by `afd_fleet/src/durable_task/input_receipt/sql/` and
-- the task input recovery reads; the reply goes with the item.
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
    mode                TEXT,
    operation_id        UUID,
    run_id              UUID,
    fencing_token       BIGINT,
    request_digest      TEXT,
    response_text       TEXT,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_items_fleet FOREIGN KEY (fleet_id)
        REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_items_source_ordinal UNIQUE (turn_id, ordinal),
    CONSTRAINT uq_durable_task_items_id_scope UNIQUE (id, fleet_id, workspace_id),
    CONSTRAINT ck_durable_task_items_receipt_all_or_none CHECK (
        num_nulls(mode, operation_id, run_id, fencing_token, request_digest, response_text) IN (0, 6))
);

CREATE INDEX idx_durable_task_items_fleet_id ON fleet.durable_task_items (fleet_id);

CREATE INDEX idx_durable_task_items_durable_task_scope ON fleet.durable_task_items (durable_task_id, fleet_id, workspace_id)
    WHERE durable_task_id IS NOT NULL;

-- One operation answers one receipt per fleet; an input's apply receipt is checked beside it.
CREATE UNIQUE INDEX uq_durable_task_items_fleet_operation ON fleet.durable_task_items (fleet_id, operation_id)
    WHERE operation_id IS NOT NULL;

CREATE FUNCTION fleet.durable_task_items_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'durable_task_items is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_items_refuse_update
    BEFORE UPDATE OF id, fleet_id, workspace_id, turn_id, ordinal, plan_digest, action,
        source_start, source_end, durable_task_id, created_at ON fleet.durable_task_items
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_items_refuse_update();

CREATE TRIGGER trg_durable_task_items_refuse_receipt
    BEFORE UPDATE OF mode, operation_id, run_id, fencing_token, request_digest, response_text
    ON fleet.durable_task_items
    FOR EACH ROW WHEN (OLD.request_digest IS NOT NULL)
    EXECUTE FUNCTION fleet.durable_task_items_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_items TO api_runtime;
GRANT UPDATE (mode, operation_id, run_id, fencing_token, request_digest, response_text, updated_at)
    ON fleet.durable_task_items TO api_runtime;
