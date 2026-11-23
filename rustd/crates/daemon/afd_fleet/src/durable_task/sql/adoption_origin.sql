SELECT i.turn_id, t.generation, t.status AS state, t.accepted_revision_id,
       r.request_digest, t.reply_provider, t.reply_address
FROM fleet.durable_tasks t
JOIN fleet.durable_task_items i ON i.id = t.originating_durable_task_item_id
LEFT JOIN thread.replay_receipts r ON r.scope = $5 AND r.operation_id = $4
WHERE t.id = $1 AND t.fleet_id = $2 AND t.workspace_id = $3
