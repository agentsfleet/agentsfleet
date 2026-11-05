-- One native upload owner freezes one key/nonce and one ciphertext descriptor.
CREATE TABLE fleet.task_objects (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_object_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    task_identity_key_id UUID NOT NULL,
    content_identity TEXT NOT NULL,
    plaintext_digest TEXT NOT NULL,
    plaintext_size BIGINT NOT NULL CHECK (plaintext_size > 0),
    kind TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    bucket_reference TEXT NOT NULL,
    -- Native quota reservations include the fixed authentication tag.
    accounted_bytes BIGINT NOT NULL CHECK (accounted_bytes > 0),
    owner_checkpoint_id UUID NOT NULL,
    owner_lease_id UUID NOT NULL,
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
    CONSTRAINT uq_task_object_workspace UNIQUE (id, workspace_id),
    CONSTRAINT fk_task_objects_identity FOREIGN KEY (task_identity_key_id, workspace_id)
        REFERENCES fleet.task_identity_keys (id, workspace_id) DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT ck_task_objects_ciphertext CHECK ((ciphertext_digest IS NULL) = (ciphertext_length IS NULL)),
    CONSTRAINT ck_task_objects_ciphertext_length CHECK (ciphertext_length IS NULL OR ciphertext_length > 0)
);
CREATE UNIQUE INDEX uq_task_objects_live_content ON fleet.task_objects
    (workspace_id, task_identity_key_id, content_identity) WHERE tombstoned_at IS NULL;
CREATE INDEX idx_task_objects_scope ON fleet.task_objects (workspace_id, task_identity_key_id);
CREATE INDEX idx_task_objects_orphans ON fleet.task_objects (updated_at, id) WHERE tombstoned_at IS NULL;
CREATE INDEX idx_task_objects_upload_owner ON fleet.task_objects (owner_checkpoint_id, id) WHERE verified_at IS NULL;
CREATE FUNCTION fleet.task_objects_record_purge() RETURNS trigger AS $$
BEGIN
    INSERT INTO fleet.task_storage_purges (task_object_id, workspace_id, task_identity_key_id, bucket_reference, accounted_bytes, stored,
        delete_after_at, next_attempt_at, created_at, updated_at)
    VALUES (OLD.id, OLD.workspace_id, OLD.task_identity_key_id, OLD.bucket_reference, OLD.accounted_bytes, OLD.verified_at IS NOT NULL,
        OLD.reclaim_after_at, OLD.reclaim_after_at, now(), now())
    ON CONFLICT (task_object_id) DO NOTHING;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_objects_record_purge BEFORE DELETE ON fleet.task_objects
    FOR EACH ROW EXECUTE FUNCTION fleet.task_objects_record_purge();
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_objects TO api_runtime;
