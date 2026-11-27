SELECT t.fleet_id
FROM thread.threads t JOIN thread.runs r ON r.fleet_id = t.fleet_id
WHERE r.id = $1 AND r.runner_id = $2
FOR SHARE OF t
