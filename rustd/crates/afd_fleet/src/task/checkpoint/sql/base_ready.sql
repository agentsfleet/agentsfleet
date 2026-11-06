UPDATE fleet.task_bases b SET manifest_root = $2, ready_at = clock_timestamp(), updated_at = now(),
    retained_bytes = (SELECT COALESCE(sum(o.accounted_bytes), 0) FROM fleet.task_object_refs r
        JOIN fleet.task_objects o ON o.id = r.task_object_id WHERE r.task_base_id = b.id)
WHERE b.id = $1::uuid AND b.ready_at IS NULL AND b.retired_at IS NULL
