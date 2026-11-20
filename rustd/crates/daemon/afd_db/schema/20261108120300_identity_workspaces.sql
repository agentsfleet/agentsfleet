-- Rows never change after creation, so UPDATE is refused. UNIQUE (id, tenant_id) lets composite
-- foreign keys prove a workspace belongs to the tenant named beside it.
CREATE TABLE identity.workspaces (
    id          UUID PRIMARY KEY,
    tenant_id   UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name        TEXT,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_workspaces_id_tenant_id UNIQUE (id, tenant_id)
);

CREATE INDEX idx_workspaces_tenant_id_created_at_id
    ON identity.workspaces (tenant_id, created_at, id);

CREATE UNIQUE INDEX uq_workspaces_tenant_id_name
    ON identity.workspaces (tenant_id, name) WHERE name IS NOT NULL;

CREATE FUNCTION identity.workspaces_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'workspaces is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_workspaces_refuse_update
    BEFORE UPDATE ON identity.workspaces
    FOR EACH ROW EXECUTE FUNCTION identity.workspaces_refuse_update();

GRANT SELECT, INSERT, DELETE ON identity.workspaces TO api_runtime;

-- Row locks need UPDATE, which api_runtime lacks here; this exact-key KEY SHARE lock is how it
-- pins a workspace before locking the workspace's mutable children.
CREATE FUNCTION identity.lock_workspace(workspace_id UUID) RETURNS TABLE (id UUID)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
    SELECT w.id FROM identity.workspaces w WHERE w.id = $1 FOR KEY SHARE OF w
$$;

REVOKE ALL ON FUNCTION identity.lock_workspace(UUID) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION identity.lock_workspace(UUID) TO api_runtime;
