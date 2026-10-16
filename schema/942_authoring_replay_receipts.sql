-- Operation identities outlive private text. A purge marks the receipt with a
-- bounded tombstone instead of releasing the operation key immediately.
CREATE TABLE IF NOT EXISTS core.authoring_replay_receipts (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_replay_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL,
    actor_id UUID NOT NULL,
    scope TEXT NOT NULL,
    operation_id UUID NOT NULL,
    request_digest TEXT NOT NULL,
    resource_id UUID NOT NULL,
    purged_at BIGINT,
    expires_at BIGINT,
    created_at BIGINT NOT NULL,
    CONSTRAINT uq_authoring_replay_scope_operation UNIQUE (scope, operation_id)
);
-- Neither workspace nor actor has a cascading FK: erasure must not turn an
-- accepted operation into a fresh create while its tombstone is still valid.
CREATE INDEX IF NOT EXISTS idx_authoring_replay_resource
    ON core.authoring_replay_receipts(resource_id);
CREATE INDEX IF NOT EXISTS idx_authoring_replay_expiry
    ON core.authoring_replay_receipts(expires_at, id) WHERE expires_at IS NOT NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON core.authoring_replay_receipts TO api_runtime;
