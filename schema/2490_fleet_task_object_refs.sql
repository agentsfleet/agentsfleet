-- Retained manifests protect objects independently of transfer-session pins.
CREATE TABLE fleet.task_object_refs (
    object_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    checkpoint_id UUID,
    base_id UUID,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_refs_object FOREIGN KEY (object_id, workspace_id)
        REFERENCES fleet.task_objects (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_task_refs_checkpoint FOREIGN KEY (checkpoint_id, workspace_id)
        REFERENCES fleet.task_checkpoints (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_task_refs_base FOREIGN KEY (base_id, workspace_id)
        REFERENCES fleet.task_bases (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT ck_task_object_refs_owner CHECK ((checkpoint_id IS NULL) <> (base_id IS NULL))
);
CREATE UNIQUE INDEX uq_task_object_refs_checkpoint ON fleet.task_object_refs (checkpoint_id, object_id)
    WHERE checkpoint_id IS NOT NULL;
CREATE UNIQUE INDEX uq_task_object_refs_base ON fleet.task_object_refs (base_id, object_id)
    WHERE base_id IS NOT NULL;
CREATE INDEX idx_task_object_refs_object ON fleet.task_object_refs (object_id);
CREATE FUNCTION fleet.task_object_refs_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'task_object_refs is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_object_refs_refuse_update BEFORE UPDATE ON fleet.task_object_refs
    FOR EACH ROW EXECUTE FUNCTION fleet.task_object_refs_refuse_update();
GRANT SELECT, INSERT, DELETE ON fleet.task_object_refs TO api_runtime;
