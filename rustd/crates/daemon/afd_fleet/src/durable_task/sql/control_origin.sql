SELECT i.turn_id AS source_turn_id, t.generation, t.reply_provider, t.reply_address
FROM fleet.durable_tasks t JOIN fleet.durable_task_items i ON i.id = t.originating_durable_task_item_id
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
