SELECT i.durable_task_id, t.generation, t.title, n.sequence, t.covered_input_sequence, t.status AS state
FROM fleet.durable_task_items i
LEFT JOIN thread.turns p ON p.id = i.turn_id
    AND p.thread_id = i.fleet_id AND p.workspace_id = i.workspace_id AND p.plan IS NOT NULL
LEFT JOIN fleet.durable_tasks t ON t.id = i.durable_task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.durable_task_inputs n ON n.durable_task_item_id = i.id AND n.durable_task_id = i.durable_task_id
WHERE i.id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
  AND i.turn_id = $4 AND i.plan_digest = $5
  AND (p.producer IS DISTINCT FROM $6
       OR connector.slack_person_allowed(i.workspace_id, p.actor, $7, $8, $9))
