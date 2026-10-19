WITH swept AS (
  UPDATE fleet.approval_gates
  SET status = $1, resolved_by = $2, detail = $3,
      resolved_at = now(), updated_at = now(), active_grant_id = NULL
  WHERE resolved_at IS NULL AND expires_at <= now()
  RETURNING id, fleet_id, event_id
)
SELECT s.id::text AS gate, s.fleet_id::text AS fleet, s.event_id AS event,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = s.fleet_id AND g.resolved_at IS NULL
           AND g.id NOT IN (SELECT id FROM swept)) AS pending
FROM swept s
