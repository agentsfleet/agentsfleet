SELECT id::text FROM authoring.attempts WHERE id = $1::uuid AND runner_id = $2::uuid FOR UPDATE
