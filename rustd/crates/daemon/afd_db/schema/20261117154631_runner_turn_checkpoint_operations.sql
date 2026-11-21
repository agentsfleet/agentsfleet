-- Opaque allocation receipts survive private plan deletion; old operations never allocate again.
CREATE TABLE runner.turn_checkpoint_operations (
    turn_checkpoint_id UUID PRIMARY KEY,
    turn_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    operation_id UUID NOT NULL,
    request_digest TEXT NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_turn_checkpoint_operation UNIQUE (turn_id, operation_id),
    -- An allocation receipt belongs to its turn and goes with it; it outlives only the plan.
    CONSTRAINT fk_turn_checkpoint_operations_turn_id FOREIGN KEY (turn_id)
        REFERENCES thread.turns(id) ON DELETE CASCADE
);
CREATE FUNCTION runner.turn_checkpoint_operations_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'turn_checkpoint_operations is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_turn_checkpoint_operations_refuse_update BEFORE UPDATE ON runner.turn_checkpoint_operations
    FOR EACH ROW EXECUTE FUNCTION runner.turn_checkpoint_operations_refuse_update();
CREATE INDEX idx_turn_operations_retention ON runner.turn_checkpoint_operations
    (created_at, turn_id, operation_id);
CREATE INDEX idx_turn_operations_turn_retention ON runner.turn_checkpoint_operations
    (turn_id, created_at, turn_checkpoint_id);

GRANT SELECT, INSERT, DELETE ON runner.turn_checkpoint_operations TO api_runtime;
