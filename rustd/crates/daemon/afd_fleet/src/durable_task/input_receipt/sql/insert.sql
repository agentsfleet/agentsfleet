INSERT INTO fleet.durable_task_input_receipts (durable_task_item_id, fleet_id, workspace_id, mode, operation_id,
    run_id, fencing_token, request_digest, response_text, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
