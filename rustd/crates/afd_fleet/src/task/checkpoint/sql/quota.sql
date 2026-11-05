INSERT INTO fleet.task_storage_quota AS q
    (workspace_id, reserved_bytes, stored_bytes, created_at, updated_at)
VALUES ($1::uuid, 0, 0, now(), now())
ON CONFLICT (workspace_id) DO UPDATE SET workspace_id = q.workspace_id
RETURNING workspace_id::text
