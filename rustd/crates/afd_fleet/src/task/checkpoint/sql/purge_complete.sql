WITH completed AS (
    UPDATE fleet.task_storage_purges p SET completed_at = clock_timestamp(), updated_at = now()
    WHERE p.task_object_id = $1::uuid AND p.workspace_id = $2::uuid AND p.completed_at IS NULL
    RETURNING p.accounted_bytes, p.stored
)
UPDATE fleet.task_storage_quota q
SET reserved_bytes = reserved_bytes - (SELECT COALESCE(sum(accounted_bytes) FILTER (WHERE NOT stored), 0) FROM completed),
    stored_bytes = stored_bytes - (SELECT COALESCE(sum(accounted_bytes) FILTER (WHERE stored), 0) FROM completed), updated_at = now()
WHERE q.workspace_id = $2::uuid
