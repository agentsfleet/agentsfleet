-- Stable private content identities are independent of master-key rewrapping.
CREATE TABLE fleet.task_identity_keys (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_identity_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
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
    retired_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_task_identity_scope UNIQUE (id, workspace_id),
    CONSTRAINT uq_task_identity_version UNIQUE (workspace_id, repository, key_version)
);
CREATE UNIQUE INDEX uq_task_identity_active ON fleet.task_identity_keys (workspace_id, repository)
    WHERE retired_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_identity_keys TO api_runtime;
