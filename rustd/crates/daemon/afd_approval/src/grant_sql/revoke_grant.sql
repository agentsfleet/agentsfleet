UPDATE connector.integration_grants g
SET status = $1, revoked_at = now(), updated_at = now()
FROM fleet.fleets z
WHERE g.id = $2
  AND g.fleet_id = $3
  AND z.id = g.fleet_id
  AND z.workspace_id = $4
  AND g.status != $1
RETURNING g.id
