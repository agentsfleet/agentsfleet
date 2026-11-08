SELECT document FROM memory.context_checkpoints
 WHERE draft_id = $1::uuid
    OR (lease_id = $2::uuid AND fleet_id = $3::uuid AND branch_id = $4::uuid)
