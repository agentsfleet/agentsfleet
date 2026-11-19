INSERT INTO runner.turn_recovery (event_id, next_generation, created_at, updated_at)
VALUES ($1, 1, now(), now())
ON CONFLICT (event_id) DO NOTHING
