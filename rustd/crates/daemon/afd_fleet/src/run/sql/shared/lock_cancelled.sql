WITH locked_slot AS MATERIALIZED (
    SELECT s.id
    FROM runner.slots s JOIN runner.runs l ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2
    FOR UPDATE OF s
)
SELECT l.cancel_requested_at IS NOT NULL AS cancelled
FROM runner.runs l JOIN locked_slot s ON s.id = COALESCE(l.fleet_id, l.draft_id)
WHERE l.id = $1 AND l.runner_id = $2
FOR UPDATE OF l
