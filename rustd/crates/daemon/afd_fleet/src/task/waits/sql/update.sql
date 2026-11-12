UPDATE fleet.task_waits SET checks = $3, baseline_digest = $4, observed_signal_sequence = $7,
    cooldown_expires_at = GREATEST(cooldown_expires_at, $5::timestamptz),
    due_at = GREATEST(cooldown_expires_at, $5::timestamptz,
        CASE WHEN signal_sequence > $7
             THEN clock_timestamp() ELSE clock_timestamp() + $6::interval END),
    claim_id = NULL, claim_expires_at = NULL, updated_at = now()
WHERE id = $1 AND claim_id = $2 AND consumed_at IS NULL AND cancelled_at IS NULL
