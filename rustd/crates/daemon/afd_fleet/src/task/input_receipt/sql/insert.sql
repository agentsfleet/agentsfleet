INSERT INTO fleet.task_input_receipts (task_item_id, fleet_id, workspace_id, mode, operation_id,
    lease_id, fencing_token, request_digest, response_text, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
