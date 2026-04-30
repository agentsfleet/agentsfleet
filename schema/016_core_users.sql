-- 017_core_users.sql
-- Identity foundation for Clerk signup bootstrap.
--
-- core.users binds an external OIDC identity (Clerk subject) to a tenant.
-- core.memberships maps users to tenants with a role. One user → one tenant
-- at signup (personal account); the many-to-many shape is forward-looking
-- for team accounts later.

CREATE TABLE IF NOT EXISTS core.users (
    user_id       UUID PRIMARY KEY,
    tenant_id     UUID NOT NULL REFERENCES core.tenants(tenant_id),
    -- Clerk user subject ("user_2aXy..."). Immutable — rotation is out of scope.
    oidc_subject  TEXT NOT NULL,
    email         TEXT NOT NULL,
    display_name  TEXT,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_users_oidc_subject
    ON core.users(oidc_subject);
CREATE INDEX IF NOT EXISTS idx_users_tenant
    ON core.users(tenant_id);

CREATE TABLE IF NOT EXISTS core.memberships (
    tenant_id   UUID NOT NULL REFERENCES core.tenants(tenant_id),
    user_id     UUID NOT NULL REFERENCES core.users(user_id),
    -- Role is a free-form lowercase label for now; enum/constraint can land
    -- with the team-accounts milestone when the role vocabulary is fixed.
    role        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_memberships_user
    ON core.memberships(user_id);
