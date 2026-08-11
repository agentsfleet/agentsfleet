-- Workspace-scoped encrypted secrets: the envelope, and the non-secret
-- projection describing it.
--
-- PRIVILEGE: `vault_runtime` owns the table grants. `api_runtime`, the role
-- every Hypertext Transfer Protocol handler runs as, holds nothing here — it
-- reaches the envelope only by assuming `vault_runtime` for the span of one
-- transaction (schema/110 membership, `WITH INHERIT FALSE, SET TRUE`), so a bug
-- in an unelevated handler is refused by PostgreSQL rather than by review.
--
-- `kek_version` carries NO DEFAULT, and that is load-bearing. A default was the
-- last way a row could be minted at a version no writer intended: every write
-- path binds the version explicitly, so the default never fired on a correct
-- write, and the only row it could produce is one a forgotten column would
-- insert silently. With the column NOT NULL and no default, that same mistake
-- fails the INSERT outright — an error instead of a row nothing can decrypt.
-- The current version stays a named application constant, never a CHECK here
-- (RULE STS); the Authenticated Encryption with Associated Data tag is the
-- failsafe underneath, so an envelope read at the wrong version fails
-- authentication rather than returning plaintext.
--
-- The `meta_` prefix marks the non-secret projection. Provider label,
-- credential kind, custom endpoint and key-presence are returned to every
-- authorized caller, so encrypting them bought no confidentiality while costing
-- a key unwrap and an envelope open per row per page view. They are written in
-- the SAME statement as the ciphertext, so the projection cannot drift from the
-- blob it describes. Nullable, no DEFAULT, no CHECK list — the kind vocabulary
-- lives in application constants (RULE STS).

CREATE TABLE IF NOT EXISTS vault.secrets (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_vault_secrets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id  UUID    NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    key_name      TEXT    NOT NULL,
    kek_version   INTEGER NOT NULL,
    encrypted_dek BYTEA   NOT NULL,
    dek_nonce     BYTEA   NOT NULL,
    dek_tag       BYTEA   NOT NULL,
    nonce         BYTEA   NOT NULL,
    ciphertext    BYTEA   NOT NULL,
    tag           BYTEA   NOT NULL,
    meta_kind     TEXT,
    meta_provider TEXT,
    meta_base_url TEXT,
    meta_has_key  BOOLEAN,
    created_at    BIGINT  NOT NULL,
    updated_at    BIGINT  NOT NULL,
    CONSTRAINT uq_vault_secrets_workspace_id_key_name UNIQUE (workspace_id, key_name)
);

-- No separate index: the unique constraint above is a btree on
-- (workspace_id, key_name), which is how every reader looks a row up, and its
-- workspace_id prefix serves the per-workspace list and the erasure cascade.
-- A second index on the same columns would only slow every write.

-- vault_runtime reads and writes the secret store; every secret path elevates
-- to it for one transaction. DELETE is included because account erasure removes
-- a workspace's secrets (`state/account_teardown.zig`). api_runtime holds no
-- direct grant — the catalogue test asserts zero rows for it here.
GRANT SELECT, INSERT, UPDATE, DELETE ON vault.secrets TO vault_runtime;

-- Read-only operator principals never see ciphertext, sealed or otherwise.
-- Stated explicitly rather than relied upon: these roles hold no grant here, and
-- this makes re-widening them a visible edit to this line.
REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_fleet;
