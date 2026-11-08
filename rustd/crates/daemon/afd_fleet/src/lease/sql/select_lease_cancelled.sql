WITH locked_slot AS MATERIALIZED (
    SELECT s.fleet_id
    FROM runner.fleet_slots s JOIN runner.leases l ON l.fleet_id = s.fleet_id
    WHERE l.id = $1 AND l.runner_id = $2
    FOR UPDATE OF s
)
SELECT l.cancel_requested_at IS NOT NULL AS cancelled
FROM runner.leases l JOIN locked_slot s ON s.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2
FOR UPDATE OF l
