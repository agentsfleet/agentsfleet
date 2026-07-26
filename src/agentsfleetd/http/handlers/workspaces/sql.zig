pub const TENANT_EXISTS =
    "SELECT 1 FROM core.tenants WHERE tenant_id = $1 LIMIT 1";

pub const INSERT_WORKSPACE =
    \\INSERT INTO core.workspaces
    \\  (workspace_id, tenant_id, name, created_by, created_at)
    \\VALUES ($1, $2, $3, $4, $5)
;
