SELECT i.admission_id, t.generation, t.status AS state, t.accepted_revision_id,
       r.request_digest, t.reply_provider, t.reply_address
FROM fleet.tasks t
JOIN fleet.task_items i ON i.id = t.originating_task_item_id
LEFT JOIN fleet.task_adoptions r ON r.task_id = t.id AND r.operation_id = $4
  AND r.fleet_id = t.fleet_id AND r.workspace_id = t.workspace_id
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
