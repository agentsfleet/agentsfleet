-- Application and reply receipts never rewrite accepted interpretation meaning.
CREATE TABLE fleet.durable_task_input_receipts (
    durable_task_item_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    mode TEXT NOT NULL,
    operation_id UUID NOT NULL,
    run_id UUID NOT NULL,
    fencing_token BIGINT NOT NULL,
    request_digest TEXT NOT NULL,
    response_text TEXT,
    created_at TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (durable_task_item_id, mode),
    CONSTRAINT uq_durable_task_input_receipts_operation UNIQUE (fleet_id, operation_id),
    CONSTRAINT fk_durable_task_input_receipts_item FOREIGN KEY (durable_task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_task_items (id, fleet_id, workspace_id) ON DELETE CASCADE
);

CREATE FUNCTION fleet.durable_task_input_receipts_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_input_receipts is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_input_receipts_refuse_update BEFORE UPDATE ON fleet.durable_task_input_receipts
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_input_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_input_receipts TO api_runtime;
