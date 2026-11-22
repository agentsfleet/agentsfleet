-- Opaque retry receipts outlive private plan retention and owner deletion.
CREATE TABLE fleet.durable_task_checkpoint_operations (
    durable_task_checkpoint_id UUID PRIMARY KEY,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    operation_id UUID NOT NULL,
    request_digest TEXT NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_durable_task_checkpoint_operation UNIQUE (durable_task_id, operation_id)
);

CREATE FUNCTION fleet.durable_task_checkpoint_operations_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_checkpoint_operations is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_checkpoint_operations_refuse_update BEFORE UPDATE ON fleet.durable_task_checkpoint_operations
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_checkpoint_operations_refuse_update();

GRANT SELECT, INSERT ON fleet.durable_task_checkpoint_operations TO api_runtime;
