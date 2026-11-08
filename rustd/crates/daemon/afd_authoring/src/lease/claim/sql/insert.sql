INSERT INTO authoring.attempts
    (id, lease_id, runner_id, workspace_id, tenant_id, user_id, draft_id, message_sequence,
     fencing_token, model_token_hash, model_snapshot, status, lease_expires_at, expires_at,
     reserved_nanos, usage_unknown, call_count, output_token_count, reported_runtime_ms,
     last_activity_sequence, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
        $9, $10, $11::jsonb, $12, $13, $14,
        $15, false, 0, 0, 0,
        0, now(), now())
