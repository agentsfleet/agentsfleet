-- Non-secret credential metadata, promoted out of the encrypted envelope (the metadata promotion).
--
-- Every field the tenant Models page displays -- provider label, credential
-- kind, custom endpoint URL, and whether a key is present -- was stored INSIDE
-- the AES-GCM envelope beside the api_key. Rendering one page therefore opened
-- one envelope per row. schema/027's header records the original decision:
-- "Provider labels, base_url, kind, and api_key remain vault JSON metadata,
-- not table columns, so one stored key can back many model rows."
--
-- Not one of these four is a secret. Each is returned to every authorized
-- caller, so encrypting them bought no confidentiality while costing a Key
-- Encryption Key unwrap and an Authenticated Encryption with Associated Data
-- open per row, per page view. Promoting them takes the library read path to
-- ZERO decryptions (the never-decrypt invariant). The api_key -- the only genuine
-- secret in the blob -- stays inside the envelope, untouched by this change.
--
-- The `meta_` prefix marks the group as the non-secret projection: a reader
-- who sees meta_* knows the column is safe to select onto a response, and that
-- anything without the prefix is envelope material that is not.
--
-- Written in the SAME statement as the ciphertext (secrets/sql.zig
-- INSERT_SECRET, reached only through vault.storeJsonPlaintext), so the
-- projection cannot drift from the blob it describes -- no code path can write
-- one without the other, and the ON CONFLICT arm updates both together.
--
-- Nullable, no DEFAULT, no CHECK list (RULE STS -- the kind value set lives in
-- Zig as secret_metadata.Kind, never as SQL string literals). NULL means the
-- row was written before this migration; `agentsfleetd backfill` fills them once
-- against a development database. The read path does NOT decrypt a
-- NULL row to heal it -- a fallback would make Invariant 5 conditional -- it
-- reports the row as an opaque custom_secret until the backfill runs.

ALTER TABLE vault.secrets
    ADD COLUMN IF NOT EXISTS meta_kind     TEXT,
    ADD COLUMN IF NOT EXISTS meta_provider TEXT,
    ADD COLUMN IF NOT EXISTS meta_base_url TEXT,
    ADD COLUMN IF NOT EXISTS meta_has_key  BOOLEAN;

-- No new GRANT (RULE SGR is satisfied by schema/002): api_runtime already holds
-- SELECT, INSERT, UPDATE on vault.secrets, and PostgreSQL table-level grants
-- cover columns added later. ops_readonly_human and ops_readonly_fleet remain
-- REVOKE'd from the table, so promoting metadata out of the envelope does not
-- widen what a read-only operator can see.
--
-- No new index: every reader of these columns looks the row up by
-- (workspace_id, key_name), which idx_vault_secrets_workspace already covers.
-- The batch presence read (state/vault.zig markExisting) widens its SELECT list
-- over that same index -- it adds columns to an existing query, not a scan.
