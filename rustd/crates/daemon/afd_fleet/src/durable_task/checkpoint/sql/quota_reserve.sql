UPDATE fleet.durable_task_storage_quota
SET reserved_bytes = reserved_bytes + $2::bigint, updated_at = now()
WHERE workspace_id = $1
