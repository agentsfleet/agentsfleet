UPDATE authoring.attempts SET status = $2, updated_at = now() WHERE id = $1::uuid
