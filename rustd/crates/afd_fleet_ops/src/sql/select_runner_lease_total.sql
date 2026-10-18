SELECT (
    SELECT count(*)
    FROM runner.leases l
    LEFT JOIN fleet.fleets f ON f.id = l.fleet_id
    WHERE l.runner_id = r.id
      AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
      AND ($3::text IS NULL OR l.fleet_id::text = $3::text OR lower(f.name) = lower($3::text))
)
FROM runner.runners r
WHERE r.id = $1::uuid
