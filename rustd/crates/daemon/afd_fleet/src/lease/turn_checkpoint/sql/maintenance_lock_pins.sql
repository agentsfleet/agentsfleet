SELECT id FROM runner.turn_checkpoint_pins WHERE turn_checkpoint_id=ANY($1::uuid[]) ORDER BY id FOR UPDATE
