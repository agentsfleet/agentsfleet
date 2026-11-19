-- The billing and ownership root. Account erasure is one DELETE here, cascading to every
-- workspace, person, model and wallet.
CREATE TABLE identity.tenants (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.tenants TO api_runtime;
