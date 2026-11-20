-- The envelope-encrypted secret plus a non-secret `meta_` projection read by every secret list;
-- one statement writes both, so they cannot drift. `kek_version` has no DEFAULT, so a writer that
-- forgets it fails instead of minting a row nothing can decrypt.
CREATE TABLE vault.secrets (
    id                 UUID PRIMARY KEY,
    workspace_id       UUID    NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    key_name           TEXT    NOT NULL,
    kek_version        INTEGER NOT NULL,
    dek_ciphertext     BYTEA   NOT NULL,
    dek_nonce          BYTEA   NOT NULL,
    dek_tag            BYTEA   NOT NULL,
    secret_nonce       BYTEA   NOT NULL,
    secret_ciphertext  BYTEA   NOT NULL,
    secret_tag         BYTEA   NOT NULL,
    meta_kind          TEXT,
    meta_provider      TEXT,
    meta_base_url      TEXT,
    meta_has_key       BOOLEAN,
    created_at         TIMESTAMPTZ(3)  NOT NULL,
    updated_at         TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_secrets_workspace_id_key_name UNIQUE (workspace_id, key_name)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON vault.secrets TO api_runtime;

REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_fleet;
