-- A never-issued native continuation has no provider usage or separate human reply.
UPDATE fleet.admissions a SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
FROM fleet.tasks t WHERE t.id = $1 AND a.task_id = t.id AND a.fleet_id = t.fleet_id
  AND a.workspace_id = t.workspace_id AND a.settled_at IS NULL AND a.producer = $2
  AND a.task_generation IS DISTINCT FROM t.generation
  AND NOT EXISTS (SELECT 1 FROM fleet.events e JOIN runner.leases l ON l.event_id = e.event_id
      WHERE e.fleet_id = a.fleet_id AND e.event_id = a.id)
