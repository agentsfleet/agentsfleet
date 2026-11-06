SELECT request_digest FROM fleet.task_input_receipts
WHERE task_item_id = $1::uuid AND mode = $2 AND fleet_id = $3::uuid AND workspace_id = $4::uuid
UNION ALL
SELECT request_digest FROM fleet.task_input_receipts
WHERE operation_id = $5::uuid AND fleet_id = $3::uuid AND workspace_id = $4::uuid
  AND NOT (task_item_id = $1::uuid AND mode = $2)
