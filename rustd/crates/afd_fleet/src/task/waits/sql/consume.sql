UPDATE fleet.task_waits SET consumed_at = clock_timestamp(), consumed_admission_id = $3::uuid,
    claim_id = NULL, claim_expires_at = NULL, updated_at = now()
WHERE id = $1::uuid AND claim_id = $2::uuid AND consumed_at IS NULL AND cancelled_at IS NULL
