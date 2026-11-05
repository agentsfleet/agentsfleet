UPDATE fleet.task_waits SET checks = $3, baseline_digest = $4, observed_signal_at = $5::timestamptz, observed_signal_sequence = $8,
    cooldown_until = GREATEST(cooldown_until, $6::timestamptz),
    due_at = GREATEST(cooldown_until, $6::timestamptz,
        CASE WHEN signal_sequence > $8
             THEN clock_timestamp() ELSE clock_timestamp() + $7::interval END),
    claim_id = NULL, claim_expires_at = NULL, updated_at = now()
WHERE id = $1::uuid AND claim_id = $2::uuid AND consumed_at IS NULL AND cancelled_at IS NULL
