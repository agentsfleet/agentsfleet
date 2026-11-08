UPDATE memory.context_checkpoints
   SET resource_version = $5, document = $6, updated_at = now()
 WHERE (draft_id = $1 OR (lease_id = $2 AND fleet_id = $3 AND branch_id = $4))
   AND resource_version = $7
