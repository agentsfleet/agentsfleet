-- The billing and ownership root. Account erasure is one DELETE here, cascading to every
-- workspace, person, model and wallet.
CREATE TABLE identity.tenants (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_tenants_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.tenants TO api_runtime;
