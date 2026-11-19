SELECT count(*)
FROM runner.turn_checkpoints
WHERE event_id = $1 AND ready_at IS NOT NULL
