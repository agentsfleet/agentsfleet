SELECT l.created_at
FROM runner.leases l
LEFT JOIN fleet.fleets f ON f.id = l.fleet_id
WHERE l.id = $1::uuid
  AND l.runner_id = $2::uuid
  AND ($3::uuid IS NULL OR l.workspace_id = $3::uuid)
  AND ($4::text IS NULL OR l.fleet_id::text = $4::text OR lower(f.name) = lower($4::text))
