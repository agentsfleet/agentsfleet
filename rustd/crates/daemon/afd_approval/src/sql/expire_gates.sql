WITH swept AS (
  UPDATE fleet.approval_gates
  SET status = $1, resolved_by = $2, detail = $3,
      resolved_at = now(), updated_at = now(), active_grant_id = NULL
  WHERE resolved_at IS NULL AND expires_at <= now()
  RETURNING id, fleet_id, turn_id
), unparked AS (
  UPDATE thread.turns a SET parked_at = NULL, updated_at = now()
  FROM swept r
  -- Parked rows and assigned ones: a park in flight holds an assigned row's lock and
  -- this waits on it, so the park cannot strand its flag. The unassigned backlog is
  -- neither rewritten nor locked.
  WHERE a.thread_id = r.fleet_id AND a.settled_at IS NULL
    AND (a.parked_at IS NOT NULL OR a.run_fence IS NOT NULL)
)

SELECT s.id AS gate, s.fleet_id AS fleet, s.turn_id AS event,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = s.fleet_id AND g.resolved_at IS NULL
           AND g.id NOT IN (SELECT id FROM swept)) AS pending
FROM swept s
