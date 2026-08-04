-- The user: an external identity subject bound to a tenant.
--
-- `oidc_subject` is the identity provider's subject claim (for example
-- `user_2aXy…`). It is opaque, provider-issued, and immutable — rotation is
-- out of scope, so nothing updates it. It is the lookup key on every
-- authenticated request, which is why it carries its own unique index rather
-- than relying on a scan.
--
-- One user belongs to one tenant at signup (a personal account). The
-- many-to-many shape lives in `core.memberships` and is forward-looking for
-- team accounts; this column is the fast path that today's single-tenant
-- signup actually reads.

CREATE TABLE IF NOT EXISTS core.users (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_users_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id     UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    oidc_subject  TEXT NOT NULL,
    email         TEXT NOT NULL,
    display_name  TEXT,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);

-- Every authenticated request resolves a subject to a user. Unique because two
-- users sharing a subject would make that resolution ambiguous, which is an
-- authentication bug rather than a data one.
CREATE UNIQUE INDEX IF NOT EXISTS uq_users_oidc_subject
    ON core.users (oidc_subject);

-- The tenant's user list, and the index the erasure cascade walks.
CREATE INDEX IF NOT EXISTS idx_users_tenant_id
    ON core.users (tenant_id);

-- api_runtime binds a subject at signup, reads the row on every authed request,
-- updates the profile, and deletes during account erasure.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.users TO api_runtime;
