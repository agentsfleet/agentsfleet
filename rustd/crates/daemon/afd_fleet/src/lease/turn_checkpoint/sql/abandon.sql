UPDATE runner.turn_checkpoints
SET abandoned_at = COALESCE(abandoned_at, clock_timestamp()), updated_at = clock_timestamp()
WHERE id = $1 AND ready_at IS NULL
