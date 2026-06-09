-- Tenant API keys — multi-key, rotatable, self-service admin tokens.
-- One row per minted zmb_t_ key. key_hash = SHA-256(raw_key) as bytea.
-- Raw key returned once at POST /v1/api-keys; never retrievable thereafter.
-- last_used_at is provisioned NULL and stays NULL until the async-stamping
-- workstream ships (see spec Out of Scope).

CREATE TABLE IF NOT EXISTS core.api_keys (
    uid           UUID        PRIMARY KEY,
    CONSTRAINT ck_api_keys_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    tenant_id     UUID        NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    key_name      TEXT        NOT NULL,
    description   TEXT        NOT NULL,                               -- optional free-text description (<=256 chars enforced in handler)
    key_hash      TEXT        NOT NULL,                               -- SHA-256 hex of raw zmb_t_ key (64 chars)
    created_by    TEXT        NOT NULL,                               -- OIDC sub of the admin who minted it (opaque provider-issued string, not a UUID)
    active        BOOLEAN     NOT NULL,
    revoked_at    BIGINT      NULL,
    last_used_at  BIGINT      NULL,
    created_at    BIGINT      NOT NULL,
    updated_at    BIGINT      NOT NULL,
    CONSTRAINT api_keys_name_per_tenant_uniq UNIQUE (tenant_id, key_name),
    CONSTRAINT api_keys_hash_uniq            UNIQUE (key_hash),
    CONSTRAINT api_keys_revoked_iff_inactive CHECK ((active = FALSE) = (revoked_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_active
    ON core.api_keys (tenant_id, active);

CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash_active
    ON core.api_keys (key_hash) WHERE active = TRUE;

GRANT SELECT, INSERT, UPDATE, DELETE ON core.api_keys TO api_runtime;
