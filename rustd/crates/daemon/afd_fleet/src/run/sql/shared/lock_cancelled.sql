WITH locked_thread AS MATERIALIZED (
    SELECT s.id
    FROM thread.threads s JOIN thread.runs l ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2
    FOR NO KEY UPDATE OF s
)
SELECT l.cancel_requested_at IS NOT NULL AS cancelled
FROM thread.runs l JOIN locked_thread s ON s.id = COALESCE(l.fleet_id, l.draft_id)
WHERE l.id = $1 AND l.runner_id = $2
FOR UPDATE OF l
