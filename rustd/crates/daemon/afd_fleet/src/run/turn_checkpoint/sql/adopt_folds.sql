UPDATE fleet.admissions
SET input_claim_run_id = $2, input_claim_fence = $3, updated_at = now()
WHERE id = ANY($1::uuid[]) AND fleet_id = $4 AND workspace_id = $5
  AND settled_at IS NULL AND parked_at IS NULL AND task_id IS NULL AND task_generation IS NULL
  AND (input_claim_fence IS NULL OR input_claim_fence <= $3)
RETURNING id
