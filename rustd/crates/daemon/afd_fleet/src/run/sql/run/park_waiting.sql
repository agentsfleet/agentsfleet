UPDATE thread.turns a SET parked_at = clock_timestamp(), updated_at = now()
WHERE a.thread_id = $1 AND a.id = $2 AND a.run_fence = $3
  AND a.settled_at IS NULL
  AND EXISTS (SELECT 1 FROM fleet.approval_gates g
    WHERE g.fleet_id = a.thread_id AND g.resolved_at IS NULL
      AND (g.turn_id IS NULL OR g.turn_id = a.id))
