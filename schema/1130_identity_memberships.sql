-- Person ↔ tenant with a role. A table, not a users column, because one person
-- belongs to many tenants. A role change is delete + insert, so rows are immutable.

CREATE TABLE identity.memberships (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_memberships_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_memberships_tenant_id_user_id UNIQUE (tenant_id, user_id)
);

-- "Which tenants am I in", read at sign-in.
CREATE INDEX idx_memberships_user_id ON identity.memberships (user_id);

CREATE FUNCTION identity.memberships_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'memberships is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_memberships_refuse_update
    BEFORE UPDATE ON identity.memberships
    FOR EACH ROW EXECUTE FUNCTION identity.memberships_refuse_update();

GRANT SELECT, INSERT, DELETE ON identity.memberships TO api_runtime;
