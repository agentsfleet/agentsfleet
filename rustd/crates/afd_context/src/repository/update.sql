UPDATE memory.context_checkpoints
   SET resource_version = $6, document = $7, updated_at = now()
 WHERE (draft_id = $2::uuid OR (lease_id = $3::uuid AND fleet_id = $4::uuid AND branch_id = $5::uuid))
   AND resource_version = $8
   AND $1::uuid IS NOT NULL
