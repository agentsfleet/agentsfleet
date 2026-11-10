SELECT id FROM authoring.attempts WHERE id = $1 AND runner_id = $2 FOR UPDATE
