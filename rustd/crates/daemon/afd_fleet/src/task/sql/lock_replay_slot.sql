SELECT s.fleet_id::text
FROM runner.fleet_slots s
JOIN runner.leases l ON l.fleet_id = s.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
FOR SHARE OF s
