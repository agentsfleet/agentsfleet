SELECT turn_checkpoint_id AS checkpoint_id, turn_id, fleet_id, workspace_id, request_digest, generation
FROM runner.turn_checkpoint_operations
WHERE turn_id = $1 AND operation_id = $2
