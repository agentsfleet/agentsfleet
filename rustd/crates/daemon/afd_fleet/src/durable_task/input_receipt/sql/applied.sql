-- An input's apply receipt; `trg_durable_task_inputs_refuse_receipt` refuses a second one.
UPDATE fleet.durable_task_inputs SET operation_id = $4, run_id = $5, fencing_token = $6,
    request_digest = $7, updated_at = now()
WHERE durable_task_item_id = $1 AND fleet_id = $2 AND workspace_id = $3
RETURNING id
