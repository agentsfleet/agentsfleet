INSERT INTO runner.decision_calls (
    id, original_runner_id, original_lease_id, source_admission_id,
    fleet_id, workspace_id, tenant_id, task_id, task_generation,
    fencing_token, operation_id, request_digest, assessment_id, assessment_key,
    attempt_number, retry_of_decision_call_id, logical_expires_at, purpose, status, provider, fleet_name,
    profile_snapshot, price_snapshot, context_snapshot, usage_sequence, reserved_nanos,
    created_at, updated_at, platform_model_id, decision_revision_id
) VALUES (
    $1, $2, $3, $4, $5, $6, $7,
    $8, $9, $10, $11, $12, $13, $14, $15, $16,
    COALESCE($17::timestamptz, $18::timestamptz + make_interval(secs => $19)),
    $20, $21, $22, $23, $24, $25, $26, $27, $28, $18::timestamptz, clock_timestamp(),
    $29, $30
)
