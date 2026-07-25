pub const TENANT_EXISTS =
    "SELECT 1 FROM core.tenants WHERE tenant_id = $1 LIMIT 1";

pub const FIND_IDEMPOTENT_CREATE =
    \\SELECT workspace_id::text, name, create_request_name, create_request_id
    \\FROM core.workspaces
    \\WHERE tenant_id = $1 AND create_idempotency_key = $2::uuid
    \\LIMIT 1
;

pub const INSERT_WORKSPACE =
    \\INSERT INTO core.workspaces
    \\  (workspace_id, tenant_id, name, created_by, created_at,
    \\   create_idempotency_key, create_request_name, create_request_id)
    \\VALUES ($1, $2, $3, $4, $5, $6::uuid, $7, $8)
;
