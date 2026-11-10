INSERT INTO fleet.task_inputs (id, task_id, fleet_id, workspace_id, task_item_id, sequence,
    accepted_text, interpretation, accepting_lease_id, accepting_fence, generation, created_at, updated_at)
VALUES ($1, $2, $3, $4, $1, $5, $6, $7, $8, $9, $10,
    clock_timestamp(), clock_timestamp())
