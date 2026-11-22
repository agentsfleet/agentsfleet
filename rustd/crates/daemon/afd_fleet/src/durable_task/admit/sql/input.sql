INSERT INTO fleet.durable_task_inputs (id, durable_task_id, fleet_id, workspace_id, durable_task_item_id, sequence,
    accepted_text, interpretation, generation, created_at, updated_at)
VALUES ($1, $2, $3, $4, $1, $5, $6, $7, $8,
    clock_timestamp(), clock_timestamp())
