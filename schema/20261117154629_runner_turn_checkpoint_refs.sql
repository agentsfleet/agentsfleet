-- Event manifests and task manifests share physical encrypted object custody.
CREATE TABLE runner.turn_checkpoint_refs (
    turn_checkpoint_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    task_object_id UUID NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (turn_checkpoint_id, task_object_id),
    CONSTRAINT fk_turn_refs_plan FOREIGN KEY (turn_checkpoint_id, workspace_id)
        REFERENCES runner.turn_checkpoints (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_turn_refs_object FOREIGN KEY (task_object_id, workspace_id)
        REFERENCES fleet.task_objects (id, workspace_id) ON DELETE CASCADE
);
CREATE INDEX idx_turn_refs_object ON runner.turn_checkpoint_refs (task_object_id);

CREATE FUNCTION runner.turn_checkpoint_refs_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'turn_checkpoint_refs is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_turn_checkpoint_refs_refuse_update BEFORE UPDATE ON runner.turn_checkpoint_refs
    FOR EACH ROW EXECUTE FUNCTION runner.turn_checkpoint_refs_refuse_update();

GRANT SELECT, INSERT, DELETE ON runner.turn_checkpoint_refs TO api_runtime;
