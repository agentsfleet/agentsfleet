-- The billing and ownership root. Every workspace, person, model and wallet hangs
-- off a tenant, so account erasure is one DELETE here (deletion matrix: Tenant).

CREATE TABLE identity.tenants (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_tenants_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    name        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.tenants TO api_runtime;
