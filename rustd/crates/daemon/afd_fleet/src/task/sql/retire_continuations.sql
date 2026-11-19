-- A never-issued native continuation has no provider usage or separate human reply.
UPDATE thread.turns a SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
FROM fleet.tasks t WHERE t.id = $1 AND a.task_id = t.id AND a.thread_id = t.fleet_id
  AND a.workspace_id = t.workspace_id AND a.settled_at IS NULL AND a.producer = $2
  AND a.task_generation IS DISTINCT FROM t.generation
  AND NOT EXISTS (SELECT 1 FROM thread.runs l
      WHERE l.fleet_id = a.thread_id AND l.event_id = a.id)
