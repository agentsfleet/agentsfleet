-- The one encrypted object custody for every checkpoint: a turn's workspace chunks and a task's
-- files and history alike. One upload owner (`owner_checkpoint_id`, opaque: equal content outlives
-- the plan that first uploaded it) freezes one key, one nonce and one ciphertext descriptor.
-- Refs from checkpoints and bases protect an object; deleting it first records its purge.
CREATE TABLE runner.checkpoint_objects (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    checkpoint_identity_key_id UUID NOT NULL,
    content_identity TEXT NOT NULL,
    plaintext_digest TEXT NOT NULL,
    plaintext_size BIGINT NOT NULL CHECK (plaintext_size > 0),
    kind TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    bucket_reference TEXT NOT NULL,
    -- Includes the fixed authentication tag.
    accounted_bytes BIGINT NOT NULL CHECK (accounted_bytes > 0),
    owner_checkpoint_id UUID NOT NULL,
    owner_run_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL,
    dek_ciphertext BYTEA NOT NULL,
    dek_nonce BYTEA NOT NULL,
    dek_tag BYTEA NOT NULL,
    secret_nonce BYTEA NOT NULL,
    secret_ciphertext BYTEA NOT NULL,
    secret_tag BYTEA NOT NULL,
    kek_version INTEGER NOT NULL,
    object_nonce BYTEA NOT NULL,
    ciphertext_digest TEXT,
    ciphertext_length BIGINT,
    reclaim_after_at TIMESTAMPTZ(3) NOT NULL,
    verified_at TIMESTAMPTZ(3),
    tombstoned_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_checkpoint_objects_id_workspace_id UNIQUE (id, workspace_id),
    CONSTRAINT fk_checkpoint_objects_identity_key FOREIGN KEY (checkpoint_identity_key_id, workspace_id)
        REFERENCES runner.checkpoint_identity_keys (id, workspace_id) DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT ck_checkpoint_objects_ciphertext CHECK ((ciphertext_digest IS NULL) = (ciphertext_length IS NULL)),
    CONSTRAINT ck_checkpoint_objects_ciphertext_length CHECK (ciphertext_length IS NULL OR ciphertext_length > 0)
);

CREATE UNIQUE INDEX uq_checkpoint_objects_live_content ON runner.checkpoint_objects
    (workspace_id, checkpoint_identity_key_id, content_identity) WHERE tombstoned_at IS NULL;

CREATE INDEX idx_checkpoint_objects_scope ON runner.checkpoint_objects (workspace_id, checkpoint_identity_key_id);

CREATE INDEX idx_checkpoint_objects_orphans ON runner.checkpoint_objects (updated_at, id) WHERE tombstoned_at IS NULL;

CREATE FUNCTION runner.checkpoint_objects_record_purge() RETURNS trigger AS $$
BEGIN
    INSERT INTO runner.checkpoint_purges (checkpoint_object_id, workspace_id, checkpoint_identity_key_id,
        bucket_reference, next_attempt_at, created_at, updated_at)
    VALUES (OLD.id, OLD.workspace_id, OLD.checkpoint_identity_key_id, OLD.bucket_reference,
        OLD.reclaim_after_at, now(), now())
    ON CONFLICT (checkpoint_object_id) DO NOTHING;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_checkpoint_objects_record_purge BEFORE DELETE ON runner.checkpoint_objects
    FOR EACH ROW EXECUTE FUNCTION runner.checkpoint_objects_record_purge();

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoint_objects TO api_runtime;
