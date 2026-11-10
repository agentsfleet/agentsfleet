SELECT request_digest FROM fleet.task_input_receipts
WHERE task_item_id = $1 AND mode = $2 AND fleet_id = $3 AND workspace_id = $4
UNION ALL
SELECT request_digest FROM fleet.task_input_receipts
WHERE operation_id = $5 AND fleet_id = $3 AND workspace_id = $4
  AND NOT (task_item_id = $1 AND mode = $2)
