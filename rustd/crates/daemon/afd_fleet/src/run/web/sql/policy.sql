SELECT f.status, f.http_egress
FROM fleet.fleets f JOIN thread.runs r ON r.fleet_id = f.id
WHERE r.id = $1 AND r.runner_id = $2
FOR SHARE OF f
