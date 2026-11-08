UPDATE connector.integration_grants g
SET status = $1, revoked_at = now(), updated_at = now()
FROM fleet.fleets z
WHERE g.id = $2::uuid
  AND g.fleet_id = $3::uuid
  AND z.id = g.fleet_id
  AND z.workspace_id = $4::uuid
  AND g.status != $1
RETURNING g.id
