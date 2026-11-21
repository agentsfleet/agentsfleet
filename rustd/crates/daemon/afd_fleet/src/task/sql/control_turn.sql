UPDATE thread.turns a SET task_id = t.id, task_generation = t.generation, updated_at = now()
FROM fleet.tasks t WHERE t.id = $1 AND a.id = $2 AND a.task_id IS NULL
  AND a.thread_id = t.fleet_id AND a.workspace_id = t.workspace_id
