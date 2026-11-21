INSERT INTO runner.turn_recovery (turn_id, next_generation, created_at, updated_at)
VALUES ($1, 1, now(), now())
ON CONFLICT (turn_id) DO NOTHING
