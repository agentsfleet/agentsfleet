INSERT INTO fleet.durable_task_items
  (id, fleet_id, workspace_id, turn_id, ordinal, plan_digest, action,
   source_start, source_end, durable_task_id, created_at)
VALUES ($1, $2, $3, $4, $5, repeat('a', 64),
        'new_work', 0, 20, $6, now())
