-- `tenant_id` is the personal account created at signup; other tenants come via memberships.
-- Rows never change after creation, so UPDATE is refused.
CREATE TABLE identity.users (
    id            UUID PRIMARY KEY,
    tenant_id     UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    oidc_subject  TEXT NOT NULL,
    email         TEXT NOT NULL,
    display_name  TEXT,
    created_at    TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX uq_users_oidc_subject ON identity.users (oidc_subject);

CREATE INDEX idx_users_tenant_id ON identity.users (tenant_id);

CREATE FUNCTION identity.users_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'users is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_users_refuse_update
    BEFORE UPDATE ON identity.users
    FOR EACH ROW EXECUTE FUNCTION identity.users_refuse_update();

GRANT SELECT, INSERT, DELETE ON identity.users TO api_runtime;

-- Row locks need UPDATE, which api_runtime lacks here. KEY SHARE holds a person against deletion
-- for the request that acts as them; `wait = false` refuses with `lock_not_available` instead of
-- queueing behind a delete.
CREATE FUNCTION identity.lock_user(user_id UUID, wait BOOLEAN) RETURNS TABLE (id UUID)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF wait THEN
        RETURN QUERY SELECT u.id FROM identity.users u WHERE u.id = user_id FOR KEY SHARE OF u;
    ELSE
        RETURN QUERY SELECT u.id FROM identity.users u WHERE u.id = user_id FOR KEY SHARE OF u NOWAIT;
    END IF;
END;
$$;

REVOKE ALL ON FUNCTION identity.lock_user(UUID, BOOLEAN) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION identity.lock_user(UUID, BOOLEAN) TO api_runtime;
