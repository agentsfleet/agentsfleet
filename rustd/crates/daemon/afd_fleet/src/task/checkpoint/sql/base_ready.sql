UPDATE fleet.task_bases b SET manifest_root = $2, ready_at = clock_timestamp(), updated_at = now()
WHERE b.id = $1 AND b.ready_at IS NULL AND b.retired_at IS NULL
