SELECT s.fleet_id
FROM runner.fleet_slots s
JOIN runner.leases l ON l.fleet_id = s.fleet_id
WHERE l.id = $1 AND l.runner_id = $2
FOR UPDATE OF s
