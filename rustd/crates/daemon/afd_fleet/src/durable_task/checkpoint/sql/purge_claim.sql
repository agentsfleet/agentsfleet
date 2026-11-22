WITH due AS MATERIALIZED (
    SELECT checkpoint_object_id FROM runner.checkpoint_purges
    WHERE completed_at IS NULL AND next_attempt_at <= statement_timestamp()
    ORDER BY next_attempt_at, checkpoint_object_id LIMIT $1 FOR UPDATE SKIP LOCKED
)
UPDATE runner.checkpoint_purges p SET next_attempt_at = clock_timestamp() + make_interval(secs => $2), updated_at = now()
FROM due WHERE p.checkpoint_object_id = due.checkpoint_object_id
RETURNING p.checkpoint_object_id, p.workspace_id, p.checkpoint_identity_key_id, p.bucket_reference
