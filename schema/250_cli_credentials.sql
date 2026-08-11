-- Command-Line Interface credentials — durable, user-scoped, one per machine.
--
-- One row per credential minted by `agentsfleet login`. `credential_hash` is
-- the SHA-256 hex of the raw value; the raw value is returned once at creation
-- and is never retrievable again, so this table holds no credential material
-- (RULE VLT). `credential_prefix` is the leading, non-secret fragment kept for
-- display, so an operator can recognise a credential in a list without the
-- table storing anything that authenticates.
--
-- `user_id` is a foreign key, which is the deliberate INVERSE of the choice
-- `240_api_keys.sql` makes for `created_by`. That table keeps a plain string
-- precisely so a tenant automation key outlives the admin who minted it —
-- erasing a departed admin must not break nightly jobs. A personal credential
-- inverts the requirement: if the human is erased, every terminal holding
-- their credential must stop working, or offboarding is theatre and a
-- credential shared with a colleague outlives the account it belongs to.
--
-- `deployment` is the API origin that minted the credential. It is stored
-- because a credential and the deployment that issued it are one fact; holding
-- them apart is what let a terminal logged into development silently reach
-- production.
--
-- `machine_name` and `created_from_address` are MINT-TIME facts, written once.
-- They are what makes credential sharing visible without any per-request
-- bookkeeping: a shared credential is minted on the sharer's own machine, so
-- it arrives as a second live row under one `user_id` carrying a different
-- `machine_name`.
--
-- `credential_hash` is a plain unsalted digest on purpose. That is safe ONLY
-- because the raw value is generated from a cryptographically secure source
-- with full entropy: a digest cannot be replayed (the server hashes what is
-- presented, so offering the digest hashes it again and matches nothing), and
-- cannot be inverted so long as the input is not guessable. A slow key
-- derivation function would be the wrong instrument here — those exist for
-- human-chosen passwords, and this lookup is the hottest read in the system.
-- The entropy requirement is therefore load-bearing, not incidental, and is
-- asserted by test rather than left to the generator's good manners.
--
-- There is no `updated_at`, and no `last_used_at`. The only mutation this row
-- ever takes is revocation, and `revoked_at` already records when that
-- happened — an `updated_at` would equal `created_at` until revocation and
-- `revoked_at` afterwards, which is the two-columns-one-fact problem `240`'s
-- CHECK exists to police. `last_used_at` is absent because nothing here reads
-- it: the question it would answer — who else holds a credential for this
-- account — is already answered at mint, since a shared credential is minted
-- on the sharer's machine and lands as a second live row under one user_id.
-- Provisioning a column for a reader that does not exist is speculative
-- (RULE NDC), and this repository rebuilds its schema from empty, so adding it
-- if a reader ever arrives costs one line.
--
-- (`240` carries `last_used_at` and DOES stamp it on the authentication path —
-- see `cmd/api_key_lookup.zig`, which mitigates the write with FOR UPDATE SKIP
-- LOCKED and a swallowed error. The comment in `240` claiming the column stays
-- NULL predates that and is stale. Noted here so this file's silence is read as
-- a decision rather than an oversight.)
--
-- Unlike `240`, there is likewise no `active` column: revocation is held once,
-- by `revoked_at`, and the partial unique index below reads it directly.

CREATE TABLE IF NOT EXISTS core.cli_credentials (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_cli_credentials_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    user_id               UUID NOT NULL REFERENCES core.users(id) ON DELETE CASCADE,
    tenant_id             UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    machine_name          TEXT NOT NULL,
    credential_hash       TEXT NOT NULL,
    credential_prefix     TEXT NOT NULL,
    deployment            TEXT NOT NULL,
    created_from_address  TEXT NOT NULL,
    created_at            BIGINT NOT NULL,
    revoked_at            BIGINT NULL,
    CONSTRAINT uq_cli_credentials_credential_hash UNIQUE (credential_hash)
);

-- One live credential per machine, enforced by the database rather than by
-- store discipline. A re-login revokes the prior row before inserting its
-- replacement; if that ordering is ever broken, the insert fails loudly here
-- instead of leaving two live credentials an operator cannot tell apart.
CREATE UNIQUE INDEX IF NOT EXISTS uq_cli_credentials_user_machine_live
    ON core.cli_credentials (user_id, machine_name)
    WHERE revoked_at IS NULL;

-- The authentication lookup filters `credential_hash` alone and never pairs it
-- with revocation state, so the unique constraint above is the whole access
-- path and no second index on that column is warranted. Revocation is checked
-- from the row once it is found.

-- A user's own credential list, for display and for the revoke-on-relogin
-- lookup, which reads by user and machine.
CREATE INDEX IF NOT EXISTS idx_cli_credentials_user_id_revoked_at
    ON core.cli_credentials (user_id, revoked_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.cli_credentials TO api_runtime;
