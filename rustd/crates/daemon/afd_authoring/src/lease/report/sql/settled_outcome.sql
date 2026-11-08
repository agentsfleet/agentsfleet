SELECT outcome FROM authoring.attempts WHERE id = $1::uuid AND outcome IS NOT NULL
