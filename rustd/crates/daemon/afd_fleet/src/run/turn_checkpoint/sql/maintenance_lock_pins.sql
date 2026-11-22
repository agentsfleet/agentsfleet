SELECT id FROM runner.checkpoint_pins WHERE checkpoint_id=ANY($1::uuid[]) ORDER BY id FOR UPDATE
