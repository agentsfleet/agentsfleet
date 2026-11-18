-- Stored as a hash only. A key outlives its creator, so `created_by_subject` is the sign-in
-- subject as text, not a user foreign key. A live key has `revoked_at IS NULL`.
CREATE TABLE identity.api_keys (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_api_keys_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id           UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL,
    key_hash            TEXT NOT NULL,
    -- NULL when the key was minted without a recoverable suffix.
    display_suffix      TEXT,
    created_by_subject  TEXT NOT NULL,
    revoked_at          TIMESTAMPTZ(3),
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_api_keys_tenant_id_name UNIQUE (tenant_id, name),
    CONSTRAINT uq_api_keys_key_hash UNIQUE (key_hash)
);

CREATE INDEX idx_api_keys_tenant_id_live
    ON identity.api_keys (tenant_id) WHERE revoked_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.api_keys TO api_runtime;
