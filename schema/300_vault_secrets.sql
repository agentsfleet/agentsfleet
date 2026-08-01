-- Workspace-scoped encrypted secrets: the envelope, and the non-secret
-- projection describing it.
--
-- PRIVILEGE: the table grants below land on `vault_runtime`, not on
-- `api_runtime`. Every Hypertext Transfer Protocol handler runs as the latter,
-- so reaching a ciphertext requires deliberately assuming the former for the
-- span of one transaction. Before this rebuild `api_runtime` held SELECT,
-- INSERT and UPDATE here directly, which meant any handler — and any bug
-- inside one — could read every stored ciphertext. The envelope was the only
-- thing in the way; now the privilege is too.
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

-- vault_runtime owns the table. api_runtime is a member (schema/100) and must
-- assume this role to reach it; it holds nothing here directly.
GRANT SELECT, INSERT, UPDATE, DELETE ON vault.secrets TO vault_runtime;

-- api_runtime reaches the NON-SECRET columns only, and the envelope not at all.
-- Wrapping every vault touch in an elevated transaction assumes each is an
-- isolated statement. Three are not: the onboarding signals
-- probe (`state/workspace_onboarding/sql.zig`) and the entry-create existence
-- check (`state/tenant_model_entries/sql.zig`) each span `vault` and `core` in
-- ONE statement, and the secrets list's metadata projection
-- (`secrets/sql.zig SELECT_METADATA_FOR_KEYS`) decrypts nothing. Elevating those
-- would put core reads inside a vault transaction to no benefit.
--
-- A column grant serves all three unelevated while `ciphertext`, `encrypted_dek`,
-- `dek_nonce`, `dek_tag`, `nonce`, `tag` and `kek_version` stay unreachable:
-- naming one as api_runtime is refused by PostgreSQL at the column, which is the
-- boundary enforced more precisely than a table grant could.
--
-- NOT covered here, and elevating on purpose: every decrypt path, every write,
-- and the whole `state/secret_reference_txn.zig` lock protocol — its step 1 is
-- `SELECT … FOR UPDATE`, which PostgreSQL requires UPDATE privilege for, so it
-- cannot ride a SELECT column grant and should not try.
GRANT SELECT (workspace_id, key_name, meta_kind, meta_provider, meta_base_url,
              meta_has_key)
    ON vault.secrets TO api_runtime;

-- Read-only operator principals never see ciphertext, sealed or otherwise.
-- Stated explicitly rather than relied upon: these roles hold no grant here, and
-- this makes re-widening them a visible edit to this line.
REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_fleet;
