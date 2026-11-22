SELECT id AS turn_id, head_turn_checkpoint_id, next_checkpoint_generation AS next_generation
FROM thread.turns
WHERE id = $1 AND next_checkpoint_generation IS NOT NULL
FOR NO KEY UPDATE
