UPDATE fleet.task_waits SET consumed_at = clock_timestamp(),
    claim_id = NULL, claim_expires_at = NULL, checks = $3, baseline_digest = $4, updated_at = now()
WHERE id = $1 AND claim_id = $2 AND consumed_at IS NULL AND cancelled_at IS NULL
