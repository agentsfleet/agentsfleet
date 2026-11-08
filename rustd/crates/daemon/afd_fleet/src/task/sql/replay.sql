SELECT i.id AS item_id, i.task_id AS task_id, i.plan_digest, i.action,
       t.status AS task_state, t.generation AS task_generation, n.sequence AS input_sequence
FROM fleet.task_items i
LEFT JOIN fleet.tasks t ON t.id = i.task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.task_inputs n ON n.task_item_id = i.id AND n.task_id = i.task_id
WHERE i.admission_id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
ORDER BY i.ordinal
