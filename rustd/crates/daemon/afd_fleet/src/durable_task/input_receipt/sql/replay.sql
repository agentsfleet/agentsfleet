SELECT request_digest FROM fleet.durable_task_input_receipts
WHERE durable_task_item_id = $1 AND mode = $2 AND fleet_id = $3 AND workspace_id = $4
UNION ALL
SELECT request_digest FROM fleet.durable_task_input_receipts
WHERE operation_id = $5 AND fleet_id = $3 AND workspace_id = $4
  AND NOT (durable_task_item_id = $1 AND mode = $2)
