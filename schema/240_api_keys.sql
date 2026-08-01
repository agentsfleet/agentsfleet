-- Tenant API keys — multi-key, rotatable, self-service admin tokens.
--
-- One row per minted key. `key_hash` is the SHA-256 hex of the raw token; the
-- raw value is returned once at creation and is never retrievable again, so
-- this table holds no credential material (RULE VLT).
--
-- `created_by` is the identity provider's subject claim of the admin who
-- minted the key — an opaque provider-issued string, deliberately not a
-- foreign key to `core.users`: the key must outlive the user row that created
-- it, or erasing a departed admin would revoke working automation.
--
-- `last_used_at` is provisioned NULL and stays NULL until asynchronous stamping
-- ships. It is not written on the authentication path on purpose: stamping
-- every request would turn an indexed read into a write on the hottest lookup
-- in the system.

CREATE TABLE IF NOT EXISTS core.api_keys (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_api_keys_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id     UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    key_name      TEXT NOT NULL,
    description   TEXT NOT NULL,
    key_hash      TEXT NOT NULL,
    created_by    TEXT NOT NULL,
    active        BOOLEAN NOT NULL,
    revoked_at    BIGINT NULL,
    last_used_at  BIGINT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT uq_api_keys_tenant_id_key_name UNIQUE (tenant_id, key_name),
    CONSTRAINT uq_api_keys_key_hash UNIQUE (key_hash),
    -- Revocation and inactivity are the same fact stated twice, so the schema
    -- refuses to hold them apart: a row cannot be inactive without a revocation
    -- instant, nor carry one while still active.
    CONSTRAINT ck_api_keys_revoked_iff_inactive
        CHECK ((active = FALSE) = (revoked_at IS NOT NULL))
);

-- The tenant's key list, filtered by active state.
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id_active
    ON core.api_keys (tenant_id, active);

-- No partial index on key_hash. The authentication lookup filters key_hash
-- alone and never pairs it with `active`, so the unique constraint above is
-- already the whole access path; the retired partial index recorded zero scans
-- against that unique's twenty thousand.

GRANT SELECT, INSERT, UPDATE, DELETE ON core.api_keys TO api_runtime;
