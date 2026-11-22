UPDATE thread.turns a SET durable_task_id = t.id, durable_task_item_id = $2, durable_task_generation = t.generation, updated_at = now()
FROM fleet.durable_tasks t WHERE t.id = $1 AND a.id = $3 AND a.durable_task_id IS NULL
  AND a.thread_id = t.fleet_id AND a.workspace_id = t.workspace_id
