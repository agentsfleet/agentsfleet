UPDATE runner.turn_recovery
SET head_turn_checkpoint_id = $2, updated_at = now()
WHERE event_id = $1 AND head_turn_checkpoint_id IS NOT DISTINCT FROM $3
