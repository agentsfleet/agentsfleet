-- One encrypted secret per (workspace, key_name): the envelope, plus a non-secret
-- projection describing it. Erasing a workspace cascades its secrets.
--
-- `kek_version` has no DEFAULT: every writer binds it, so a forgotten column fails
-- the INSERT instead of minting a row nothing can decrypt. The current version is
-- an application constant, never a CHECK (RULE STS).
--
-- The `meta_` columns are the non-secret projection (rule 5 exception): provider,
-- credential kind, custom endpoint and key presence, read by every secret list.
-- They are written in the same statement as the ciphertext, so they cannot drift
-- from the envelope they describe.

CREATE TABLE vault.secrets (
    id                 UUID PRIMARY KEY,
    CONSTRAINT ck_secrets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
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
    created_at         BIGINT  NOT NULL,
    updated_at         BIGINT  NOT NULL,
    -- Every reader looks a secret up by (workspace_id, key_name); the btree's
    -- workspace prefix also serves the per-workspace list and the erasure cascade.
    CONSTRAINT uq_secrets_workspace_id_key_name UNIQUE (workspace_id, key_name)
);

-- api_runtime holds the grants directly; narrowing them to an assumed vault role
-- is its own milestone, because the revoke and the elevation ship together.
GRANT SELECT, INSERT, UPDATE, DELETE ON vault.secrets TO api_runtime;
REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_fleet;
