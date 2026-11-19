SELECT count(*)
FROM runner.turn_checkpoints
WHERE turn_id = $1 AND ready_at IS NOT NULL
