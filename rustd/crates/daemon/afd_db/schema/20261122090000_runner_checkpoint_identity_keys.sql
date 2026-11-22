-- Stable private content identities for checkpoint objects, independent of master-key rewrapping.
-- Turn and task checkpoints of one workspace/repository share a key, so equal content has one
-- identity. Rows are written once and removed after retention; the workspace storage lock
-- serializes both.
CREATE TABLE runner.checkpoint_identity_keys (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    repository TEXT NOT NULL,
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    dek_ciphertext BYTEA NOT NULL,
    dek_nonce BYTEA NOT NULL,
    dek_tag BYTEA NOT NULL,
    secret_nonce BYTEA NOT NULL,
    secret_ciphertext BYTEA NOT NULL,
    secret_tag BYTEA NOT NULL,
    kek_version INTEGER NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_checkpoint_identity_keys_id_workspace_id UNIQUE (id, workspace_id),
    CONSTRAINT uq_checkpoint_identity_keys_version UNIQUE (workspace_id, repository, key_version)
);

CREATE UNIQUE INDEX uq_checkpoint_identity_keys_active ON runner.checkpoint_identity_keys (workspace_id, repository);

CREATE INDEX idx_checkpoint_identity_keys_retention ON runner.checkpoint_identity_keys (created_at, id);

CREATE FUNCTION runner.checkpoint_identity_keys_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'checkpoint_identity_keys is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_checkpoint_identity_keys_refuse_update
    BEFORE UPDATE ON runner.checkpoint_identity_keys
    FOR EACH ROW EXECUTE FUNCTION runner.checkpoint_identity_keys_refuse_update();

GRANT SELECT, INSERT, DELETE ON runner.checkpoint_identity_keys TO api_runtime;
