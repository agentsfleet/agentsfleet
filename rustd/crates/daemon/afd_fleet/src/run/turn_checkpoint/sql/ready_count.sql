SELECT count(*)
FROM runner.checkpoints
WHERE turn_id = $1 AND ready_at IS NOT NULL
