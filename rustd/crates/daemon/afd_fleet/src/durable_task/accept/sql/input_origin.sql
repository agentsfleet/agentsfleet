SELECT i.turn_id AS source, t.generation, t.status AS state, t.reply_provider, t.reply_address,
       EXISTS (SELECT 1 FROM thread.turns a WHERE a.durable_task_id = t.id AND a.durable_task_generation = t.generation AND a.settled_at IS NULL) AS queued
FROM fleet.durable_tasks t JOIN fleet.durable_task_items i ON i.id = t.originating_durable_task_item_id
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
