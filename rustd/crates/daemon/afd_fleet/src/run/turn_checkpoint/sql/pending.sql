SELECT id
FROM runner.checkpoints
WHERE turn_id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
FOR UPDATE
