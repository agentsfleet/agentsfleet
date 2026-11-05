INSERT INTO fleet.task_effects (id, task_id, fleet_id, workspace_id, source_item_id,
    original_lease_id, original_runner_id, original_fence, task_generation, stop_generation,
    ordinal, operation_id, covered_input_sequence, expected_head, tested_tree, request_digest,
    issuance_deadline, repository_id, installation_id, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6::uuid, $7::uuid, $8, $9, $10,
    $11, $12::uuid, $13, $14, $15, $16, clock_timestamp() + make_interval(secs => $17), $18, $19, now())
