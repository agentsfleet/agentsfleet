WITH due AS MATERIALIZED (
    SELECT durable_task_object_id FROM fleet.durable_task_storage_purges
    WHERE completed_at IS NULL AND next_attempt_at <= statement_timestamp()
    ORDER BY next_attempt_at, durable_task_object_id LIMIT $1 FOR UPDATE SKIP LOCKED
)
UPDATE fleet.durable_task_storage_purges p SET next_attempt_at = clock_timestamp() + make_interval(secs => $2), updated_at = now()
FROM due WHERE p.durable_task_object_id = due.durable_task_object_id
RETURNING p.durable_task_object_id, p.workspace_id, p.durable_task_identity_key_id, p.bucket_reference
