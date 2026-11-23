-- The remote delete owed for one removed checkpoint object. Only its opaque storage locator
-- survives, and it has no foreign key: it must outlive the object, its owner and its workspace
-- until the bucket delete is acknowledged. `runner.checkpoint_objects`' delete trigger writes it.
CREATE TABLE runner.checkpoint_purges (
    checkpoint_object_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    checkpoint_identity_key_id UUID NOT NULL,
    bucket_reference TEXT NOT NULL,
    next_attempt_at TIMESTAMPTZ(3) NOT NULL,
    completed_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_checkpoint_purges_due ON runner.checkpoint_purges (next_attempt_at, checkpoint_object_id)
    WHERE completed_at IS NULL;

CREATE INDEX idx_checkpoint_purges_identity ON runner.checkpoint_purges (checkpoint_identity_key_id)
    WHERE completed_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoint_purges TO api_runtime;
