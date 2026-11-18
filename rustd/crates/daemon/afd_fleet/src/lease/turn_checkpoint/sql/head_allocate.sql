UPDATE runner.turn_recovery
SET next_generation = next_generation + 1, updated_at = clock_timestamp()
WHERE event_id = $1 AND head_turn_checkpoint_id IS NOT DISTINCT FROM $2 AND next_generation < $3
RETURNING head_turn_checkpoint_id, next_generation - 1 AS generation
