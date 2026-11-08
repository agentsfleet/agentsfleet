UPDATE fleet.admissions a SET task_id = t.id, task_generation = t.generation, updated_at = now()
FROM fleet.tasks t WHERE t.id = $1::uuid AND a.id = $2::uuid AND a.task_id IS NULL
  AND a.fleet_id = t.fleet_id AND a.workspace_id = t.workspace_id
