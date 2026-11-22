-- The billing and ownership root. Account erasure is one DELETE here, cascading to every
-- workspace, person, model and wallet. Rows never change after creation, so UPDATE is refused.
CREATE TABLE identity.tenants (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL
);

CREATE FUNCTION identity.tenants_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'tenants is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tenants_refuse_update
    BEFORE UPDATE ON identity.tenants
    FOR EACH ROW EXECUTE FUNCTION identity.tenants_refuse_update();

GRANT SELECT, INSERT, DELETE ON identity.tenants TO api_runtime;

-- Row locks need UPDATE, which api_runtime lacks here. Model selection writes for one tenant run
-- one at a time behind this lock; NO KEY UPDATE leaves the foreign-key share locks free.
CREATE FUNCTION identity.lock_tenant(tenant_id UUID) RETURNS TABLE (id UUID)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
    SELECT t.id FROM identity.tenants t WHERE t.id = $1 FOR NO KEY UPDATE OF t
$$;

REVOKE ALL ON FUNCTION identity.lock_tenant(UUID) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION identity.lock_tenant(UUID) TO api_runtime;
