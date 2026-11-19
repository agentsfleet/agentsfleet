SELECT id
FROM runner.turn_checkpoints
WHERE event_id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
FOR UPDATE
