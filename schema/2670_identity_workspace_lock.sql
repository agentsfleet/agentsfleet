-- Lease/report and checkpoint transactions must retain their immutable workspace
-- before locking mutable children. Row locking needs UPDATE privilege, which
-- api_runtime deliberately lacks on workspaces. Expose only an exact-key lock;
-- the caller still proves fleet/lease authority and no workspace mutation is added.
CREATE FUNCTION identity.lock_workspace(workspace_id UUID) RETURNS TABLE (id UUID)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
    SELECT w.id FROM identity.workspaces w WHERE w.id = $1 FOR KEY SHARE OF w
$$;
REVOKE ALL ON FUNCTION identity.lock_workspace(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION identity.lock_workspace(UUID) TO api_runtime;
