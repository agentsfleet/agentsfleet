SELECT i.admission_id AS source_admission_id, t.generation, t.reply_provider, t.reply_address
FROM fleet.tasks t JOIN fleet.task_items i ON i.id = t.originating_task_item_id
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
