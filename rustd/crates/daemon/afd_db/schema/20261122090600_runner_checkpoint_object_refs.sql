-- A retained manifest's edge to an object it needs, from a checkpoint (turn or task) or from a
-- retained base, exactly one of the two. Edges protect objects independently of transfer pins;
-- a collector tombstones only an object no edge names.
CREATE TABLE runner.checkpoint_object_refs (
    checkpoint_object_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    checkpoint_id UUID,
    checkpoint_base_id UUID,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_checkpoint_object_refs_object FOREIGN KEY (checkpoint_object_id, workspace_id)
        REFERENCES runner.checkpoint_objects (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_checkpoint_object_refs_checkpoint FOREIGN KEY (checkpoint_id, workspace_id)
        REFERENCES runner.checkpoints (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_checkpoint_object_refs_base FOREIGN KEY (checkpoint_base_id, workspace_id)
        REFERENCES runner.checkpoint_bases (id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT ck_checkpoint_object_refs_one_owner CHECK (num_nonnulls(checkpoint_id, checkpoint_base_id) = 1)
);

CREATE UNIQUE INDEX uq_checkpoint_object_refs_checkpoint ON runner.checkpoint_object_refs
    (checkpoint_id, checkpoint_object_id) WHERE checkpoint_id IS NOT NULL;

CREATE UNIQUE INDEX uq_checkpoint_object_refs_base ON runner.checkpoint_object_refs
    (checkpoint_base_id, checkpoint_object_id) WHERE checkpoint_base_id IS NOT NULL;

CREATE INDEX idx_checkpoint_object_refs_object ON runner.checkpoint_object_refs (checkpoint_object_id);

CREATE FUNCTION runner.checkpoint_object_refs_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'checkpoint_object_refs is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_checkpoint_object_refs_refuse_update BEFORE UPDATE ON runner.checkpoint_object_refs
    FOR EACH ROW EXECUTE FUNCTION runner.checkpoint_object_refs_refuse_update();

GRANT SELECT, INSERT, DELETE ON runner.checkpoint_object_refs TO api_runtime;
