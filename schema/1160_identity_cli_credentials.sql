-- One live CLI login per machine, stored as a hash only. A table, not an api_keys
-- row, because it belongs to a person and dies with them (deletion matrix: User).
-- `revoked_at` is the NULL marker the one-live-login index reads. The metadata
-- columns are pending their reader check (postgres_schemas.md §Settled).

CREATE TABLE identity.cli_credentials (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_cli_credentials_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    user_id               UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id             UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    machine_name          TEXT NOT NULL,
    credential_hash       TEXT NOT NULL,
    credential_prefix     TEXT NOT NULL,
    deployment            TEXT NOT NULL,
    created_from_address  TEXT NOT NULL,
    revoked_at            BIGINT,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT uq_cli_credentials_credential_hash UNIQUE (credential_hash)
);

-- A new login on the same machine revokes the old one first.
CREATE UNIQUE INDEX uq_cli_credentials_user_id_machine_name_live
    ON identity.cli_credentials (user_id, machine_name) WHERE revoked_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.cli_credentials TO api_runtime;
