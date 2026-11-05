UPDATE fleet.admissions a
SET parked_at = NULL, updated_at = now()
WHERE a.fleet_id = ANY($1::uuid[])
  AND a.parked_at IS NOT NULL AND a.settled_at IS NULL
  AND NOT EXISTS (
    SELECT 1 FROM fleet.approval_gates g
    WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL AND g.event_id IS NULL)
  AND NOT EXISTS (
    SELECT 1 FROM fleet.approval_gates g
    WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
      AND g.event_id = a.id::text AND g.expires_at > statement_timestamp())
RETURNING a.fleet_id::text AS fleet_id
