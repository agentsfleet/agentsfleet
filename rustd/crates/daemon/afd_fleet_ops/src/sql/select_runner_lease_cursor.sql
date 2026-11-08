SELECT l.created_at
FROM runner.leases l
LEFT JOIN fleet.fleets f ON f.id = l.fleet_id
WHERE l.id = $1
  AND l.runner_id = $2
  AND (l.workspace_id = $3 OR $3 IS NULL)
  AND ($4::text IS NULL OR l.fleet_id = $5 OR lower(f.name) = lower($4::text))
