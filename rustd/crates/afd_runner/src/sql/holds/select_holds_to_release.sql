SELECT listed::text AS fleet_id
FROM unnest(($2::text[])::uuid[]) AS listed
WHERE NOT EXISTS (
  SELECT 1
  FROM runner.fleet_slots s
  JOIN fleet.fleets f ON f.id = s.fleet_id
  WHERE s.fleet_id = listed AND s.last_runner_id = $1::uuid AND f.status = $3::text
)
