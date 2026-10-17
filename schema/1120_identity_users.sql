-- One person, bound to one sign-in subject. `tenant_id` is the personal account
-- created at signup; membership in other tenants lives in `identity.memberships`.

CREATE TABLE identity.users (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_users_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id     UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    oidc_subject  TEXT NOT NULL,
    email         TEXT NOT NULL,
    display_name  TEXT,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);

-- Sign-in resolves the subject to a user on every authenticated request.
CREATE UNIQUE INDEX uq_users_oidc_subject ON identity.users (oidc_subject);
CREATE INDEX idx_users_tenant_id ON identity.users (tenant_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.users TO api_runtime;
