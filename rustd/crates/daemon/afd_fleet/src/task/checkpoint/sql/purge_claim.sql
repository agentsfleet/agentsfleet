WITH due AS MATERIALIZED (
    SELECT task_object_id FROM fleet.task_storage_purges
    WHERE completed_at IS NULL AND next_attempt_at <= statement_timestamp()
    ORDER BY next_attempt_at, task_object_id LIMIT $1 FOR UPDATE SKIP LOCKED
)
UPDATE fleet.task_storage_purges p SET next_attempt_at = clock_timestamp() + make_interval(secs => $2), updated_at = now()
FROM due WHERE p.task_object_id = due.task_object_id
RETURNING p.task_object_id::text, p.workspace_id::text, p.task_identity_key_id::text, p.bucket_reference
