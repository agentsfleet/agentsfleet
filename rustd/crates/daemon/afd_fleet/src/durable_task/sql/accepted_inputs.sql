SELECT i.turn_id AS source_turn_id, i.plan_digest, i.id AS item_id,
       t.id AS durable_task_id, t.generation, n.sequence, i.action,
       p.producer AS source_producer, p.request_json::text AS source_request_json,
       n.accepted_text, n.interpretation,
       n.sequence <= t.covered_input_sequence AS applied,
       (SELECT r.response_text FROM fleet.durable_task_input_receipts r WHERE r.durable_task_item_id = i.id AND r.response_text IS NOT NULL) AS response_text
FROM fleet.durable_tasks t
LEFT JOIN runner.checkpoints c ON c.id = t.checkpoint_id AND c.durable_task_id = t.id
JOIN fleet.durable_task_inputs n ON n.durable_task_id = t.id AND n.fleet_id = t.fleet_id AND n.workspace_id = t.workspace_id
JOIN fleet.durable_task_items i ON i.id = n.durable_task_item_id AND i.fleet_id = n.fleet_id AND i.workspace_id = n.workspace_id
JOIN thread.turns p ON p.id = i.turn_id AND p.plan_digest = i.plan_digest
    AND p.thread_id = i.fleet_id AND p.workspace_id = i.workspace_id
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
  AND n.sequence > COALESCE($5::bigint, c.covered_input_sequence, 0)
ORDER BY n.sequence LIMIT $4
