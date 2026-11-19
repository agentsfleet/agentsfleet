SELECT event_id, head_turn_checkpoint_id, next_generation
FROM runner.turn_recovery
WHERE event_id = $1
FOR UPDATE
