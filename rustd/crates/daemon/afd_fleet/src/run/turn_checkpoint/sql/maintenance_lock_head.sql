SELECT id AS turn_id FROM thread.turns WHERE id=$1 AND next_checkpoint_generation IS NOT NULL FOR NO KEY UPDATE NOWAIT
