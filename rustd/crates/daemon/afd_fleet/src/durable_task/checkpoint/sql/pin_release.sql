UPDATE runner.checkpoint_pins SET released_at = clock_timestamp(), updated_at = now() WHERE id = $1
