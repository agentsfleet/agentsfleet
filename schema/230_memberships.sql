-- User-to-tenant membership with a role.
--
-- Today every user has exactly one membership, created at signup, because a
-- personal account is one user in one tenant. The table exists as its own
-- many-to-many relation rather than as a column on `core.users` so that team
-- accounts do not require re-cutting identity later — the shape is already
-- right, only the row count changes.
--
-- `role` is a free-form lowercase label. It stays TEXT with no CHECK: the role
-- vocabulary lives in application constants, and SQL cannot reference them, so
-- a value list here would drift the moment the vocabulary changed (RULE STS).
-- It gains a constraint when the team-accounts milestone fixes the vocabulary.
--
-- No `updated_at`: a membership is created and revoked, never edited. Changing
-- a role is a delete plus an insert, so the audit trail is the row's existence.

CREATE TABLE IF NOT EXISTS core.memberships (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_memberships_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES core.users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    CONSTRAINT uq_memberships_tenant_id_user_id UNIQUE (tenant_id, user_id)
);

-- "Which tenants does this user belong to" — the reverse of the unique
-- constraint above, which already covers the tenant-first direction as a prefix.
CREATE INDEX IF NOT EXISTS idx_memberships_user_id
    ON core.memberships (user_id);

-- api_runtime creates the membership at signup and removes it during erasure.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.memberships TO api_runtime;
