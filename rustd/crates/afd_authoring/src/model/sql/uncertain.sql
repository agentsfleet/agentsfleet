UPDATE authoring.attempts
   SET usage_unknown = true, status = $2, updated_at = now()
 WHERE id = $1::uuid
