-- Opaque allocation receipts survive private plan deletion; old operations never allocate again.
CREATE TABLE runner.turn_checkpoint_operations (
    turn_checkpoint_id UUID PRIMARY KEY,
    CONSTRAINT ck_turn_operation_checkpoint_uuidv7 CHECK (uuid_extract_version(turn_checkpoint_id) IS NOT DISTINCT FROM 7),
    event_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    operation_id UUID NOT NULL,
    CONSTRAINT ck_turn_operation_id_uuidv7 CHECK (uuid_extract_version(operation_id) IS NOT DISTINCT FROM 7),
    request_digest TEXT NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_turn_checkpoint_operation UNIQUE (event_id, operation_id)
);
CREATE FUNCTION runner.turn_checkpoint_operations_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'turn_checkpoint_operations is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_turn_checkpoint_operations_refuse_update BEFORE UPDATE ON runner.turn_checkpoint_operations
    FOR EACH ROW EXECUTE FUNCTION runner.turn_checkpoint_operations_refuse_update();
CREATE INDEX idx_turn_operations_retention ON runner.turn_checkpoint_operations
    (created_at, event_id, operation_id);
CREATE INDEX idx_turn_operations_event_retention ON runner.turn_checkpoint_operations
    (event_id, created_at, turn_checkpoint_id);

GRANT SELECT, INSERT, DELETE ON runner.turn_checkpoint_operations TO api_runtime;

