SELECT document FROM memory.context_checkpoints
 WHERE draft_id = $1
    OR (lease_id = $2 AND fleet_id = $3 AND branch_id = $4)
