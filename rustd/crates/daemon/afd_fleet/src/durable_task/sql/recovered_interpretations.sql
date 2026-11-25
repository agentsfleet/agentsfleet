SELECT i.turn_id AS source_turn_id, i.plan_digest,
       p.producer AS source_producer, p.request_json::text AS source_request_json,
       i.id AS item_id, i.durable_task_id, t.generation, t.status AS state,
       n.sequence AS input_sequence, i.action, i.source_start, i.source_end,
       COALESCE(n.interpretation, $4) AS interpretation,
       i.response_text
FROM thread.turns p
JOIN fleet.durable_task_items i ON i.turn_id = p.id AND i.fleet_id = p.thread_id AND i.workspace_id = p.workspace_id
LEFT JOIN fleet.durable_tasks t ON t.id = i.durable_task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.durable_task_inputs n ON n.durable_task_item_id = i.id AND n.durable_task_id = i.durable_task_id
WHERE p.id = ANY($1) AND p.thread_id = $2 AND p.workspace_id = $3 AND p.plan IS NOT NULL
ORDER BY i.turn_id, i.ordinal
