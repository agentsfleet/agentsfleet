SELECT i.id AS item_id, i.durable_task_id AS durable_task_id, i.plan_digest, i.action,
       t.status AS durable_task_state, t.generation AS durable_task_generation, n.sequence AS input_sequence
FROM fleet.durable_task_items i
LEFT JOIN fleet.durable_tasks t ON t.id = i.durable_task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.durable_task_inputs n ON n.durable_task_item_id = i.id AND n.durable_task_id = i.durable_task_id
WHERE i.turn_id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
ORDER BY i.ordinal
