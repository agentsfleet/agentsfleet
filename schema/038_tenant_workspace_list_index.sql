-- 038_tenant_workspace_list_index.sql
-- Supports stable tenant workspace cursor pagination and exact-name recovery.

CREATE INDEX IF NOT EXISTS idx_workspaces_tenant_created
    ON core.workspaces(tenant_id, created_at, workspace_id);
