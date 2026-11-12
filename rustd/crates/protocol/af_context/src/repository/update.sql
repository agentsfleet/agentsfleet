UPDATE memory.context_checkpoints
   SET resource_version = $2, document = $3, updated_at = now()
 WHERE draft_id = $1 AND resource_version = $4
