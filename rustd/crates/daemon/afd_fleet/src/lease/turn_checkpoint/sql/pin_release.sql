UPDATE runner.turn_checkpoint_pins
SET released_at = COALESCE(released_at, clock_timestamp()), updated_at = now()
WHERE id = $1 AND owner_lease_id = $2 AND owner_fence = $3
