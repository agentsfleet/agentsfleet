INSERT INTO runner.turn_checkpoint_operations
    (turn_checkpoint_id, turn_id, fleet_id, workspace_id, operation_id,
     request_digest, generation, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now())
