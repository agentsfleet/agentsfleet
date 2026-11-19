SELECT turn_id, head_turn_checkpoint_id, next_generation
FROM runner.turn_recovery
WHERE turn_id = $1
FOR UPDATE
