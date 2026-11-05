WITH due AS MATERIALIZED (
    SELECT object_id FROM fleet.task_storage_purges
    WHERE completed_at IS NULL AND next_attempt_at <= statement_timestamp()
    ORDER BY next_attempt_at, object_id LIMIT $1 FOR UPDATE SKIP LOCKED
)
UPDATE fleet.task_storage_purges p SET next_attempt_at = clock_timestamp() + make_interval(secs => $2), updated_at = now()
FROM due WHERE p.object_id = due.object_id
RETURNING p.object_id::text, p.workspace_id::text, p.identity_key_id::text, p.bucket_reference
