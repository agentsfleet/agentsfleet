INSERT INTO runner.turn_recovery (event_id, next_generation, created_at, updated_at)
VALUES ($1, 1, clock_timestamp(), clock_timestamp())
ON CONFLICT (event_id) DO NOTHING
