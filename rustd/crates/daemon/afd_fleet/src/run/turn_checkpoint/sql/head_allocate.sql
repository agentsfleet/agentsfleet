UPDATE thread.turns
SET next_checkpoint_generation = next_checkpoint_generation + 1
WHERE id = $1 AND head_checkpoint_id IS NOT DISTINCT FROM $2 AND next_checkpoint_generation < $3
RETURNING head_checkpoint_id, next_checkpoint_generation - 1 AS generation
