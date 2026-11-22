-- Retained manifests protect objects independently of transfer-session pins.
CREATE TABLE fleet.durable_task_object_refs (
    durable_task_object_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    durable_task_checkpoint_id UUID,
    durable_task_base_id UUID,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_refs_object FOREIGN KEY (durable_task_object_id, workspace_id)
        REFERENCES fleet.durable_task_objects (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_durable_task_refs_checkpoint FOREIGN KEY (durable_task_checkpoint_id, workspace_id)
        REFERENCES fleet.durable_task_checkpoints (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_durable_task_refs_base FOREIGN KEY (durable_task_base_id, workspace_id)
        REFERENCES fleet.durable_task_bases (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT ck_durable_task_object_refs_owner CHECK ((durable_task_checkpoint_id IS NULL) <> (durable_task_base_id IS NULL))
);

CREATE UNIQUE INDEX uq_durable_task_object_refs_checkpoint ON fleet.durable_task_object_refs (durable_task_checkpoint_id, durable_task_object_id)
    WHERE durable_task_checkpoint_id IS NOT NULL;

CREATE UNIQUE INDEX uq_durable_task_object_refs_base ON fleet.durable_task_object_refs (durable_task_base_id, durable_task_object_id)
    WHERE durable_task_base_id IS NOT NULL;

CREATE INDEX idx_durable_task_object_refs_object ON fleet.durable_task_object_refs (durable_task_object_id);

CREATE FUNCTION fleet.durable_task_object_refs_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_object_refs is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_object_refs_refuse_update BEFORE UPDATE ON fleet.durable_task_object_refs
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_object_refs_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_object_refs TO api_runtime;
