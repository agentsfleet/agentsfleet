INSERT INTO fleet.task_items
  (id, fleet_id, workspace_id, admission_id, ordinal, plan_digest, action,
   source_start, source_end, task_id, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
