UPDATE thread.turns
SET head_turn_checkpoint_id = $2
WHERE id = $1 AND next_checkpoint_generation IS NOT NULL AND head_turn_checkpoint_id IS NOT DISTINCT FROM $3
