-- One live CLI login per machine, stored as a hash only. It belongs to a person, so it is
-- deleted with the user. A live login has `revoked_at IS NULL`.
CREATE TABLE identity.cli_credentials (
    id                    UUID PRIMARY KEY,
    user_id               UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id             UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    machine_name          TEXT NOT NULL,
    credential_hash       TEXT NOT NULL,
    revoked_at            TIMESTAMPTZ(3),
    created_at            TIMESTAMPTZ(3) NOT NULL,
    updated_at            TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_cli_credentials_credential_hash UNIQUE (credential_hash)
);

-- A new login on the same machine revokes the old one first.
CREATE UNIQUE INDEX uq_cli_credentials_user_id_machine_name_live
    ON identity.cli_credentials (user_id, machine_name) WHERE revoked_at IS NULL;

-- A user or tenant delete cascades here; these find its revoked rows too,
-- which the live-only unique index above cannot.
CREATE INDEX idx_cli_credentials_user_id ON identity.cli_credentials (user_id);

CREATE INDEX idx_cli_credentials_tenant_id ON identity.cli_credentials (tenant_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.cli_credentials TO api_runtime;
