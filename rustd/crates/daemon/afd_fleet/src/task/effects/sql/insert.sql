INSERT INTO fleet.task_effects (id, task_id, fleet_id, workspace_id, source_item_id,
    original_lease_id, original_runner_id, original_fence, task_generation, stop_generation,
    ordinal, operation_id, covered_input_sequence, expected_head, tested_tree, request_digest,
    issuance_expires_at, repository_id, installation_id, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
    $11, $12, $13, $14, $15, $16, clock_timestamp() + make_interval(secs => $17), $18, $19, now())
