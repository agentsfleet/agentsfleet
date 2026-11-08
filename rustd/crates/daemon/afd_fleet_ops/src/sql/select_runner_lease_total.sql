SELECT (
    SELECT count(*)
    FROM runner.leases l
    LEFT JOIN fleet.fleets f ON f.id = l.fleet_id
    WHERE l.runner_id = r.id
      AND (l.workspace_id = $2 OR $2 IS NULL)
      AND ($3::text IS NULL OR l.fleet_id = $4 OR lower(f.name) = lower($3::text))
)
FROM runner.runners r
WHERE r.id = $1
