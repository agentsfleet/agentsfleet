UPDATE authoring.attempts SET call_count = call_count + 1, updated_at = now() WHERE id = $1::uuid
