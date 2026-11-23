UPDATE thread.turns SET next_checkpoint_generation = 1
WHERE id = $1 AND next_checkpoint_generation IS NULL
