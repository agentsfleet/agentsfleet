UPDATE fleet.task_storage_quota
SET reserved_bytes = reserved_bytes + $2::bigint, updated_at = now()
WHERE workspace_id = $1::uuid AND reserved_bytes + stored_bytes <= $3::bigint - $2::bigint
