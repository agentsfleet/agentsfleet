INSERT INTO fleet.task_effect_states (effect_id, status, created_at, updated_at) VALUES ($1::uuid, $2, now(), now())
