SELECT s.fleet_id::text AS fleet_id
FROM runner.fleet_slots s
WHERE s.fleet_id IN (
  SELECT a.fleet_id FROM fleet.admissions a
  WHERE a.parked_at IS NOT NULL AND a.settled_at IS NULL
    AND NOT EXISTS (
      SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL AND g.event_id IS NULL)
    AND NOT EXISTS (
      SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
        AND g.event_id = a.id::text AND g.expires_at > statement_timestamp()))
ORDER BY s.fleet_id
LIMIT $1
FOR UPDATE OF s SKIP LOCKED
