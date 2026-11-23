SELECT i.response_text FROM fleet.durable_task_items i
WHERE i.turn_id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
ORDER BY i.ordinal
