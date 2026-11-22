-- An item's reply receipt; `trg_durable_task_items_refuse_receipt` refuses a second one.
UPDATE fleet.durable_task_items SET mode = $4, operation_id = $5, run_id = $6, fencing_token = $7,
    request_digest = $8, response_text = $9, updated_at = now()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
RETURNING id
