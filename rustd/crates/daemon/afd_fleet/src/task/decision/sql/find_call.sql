SELECT id::text, original_runner_id::text, original_lease_id::text,
       source_admission_id::text, fleet_id::text, workspace_id::text, tenant_id::text,
       task_id::text, fencing_token, request_digest, assessment_id::text,
       assessment_key, attempt_number, retry_of_decision_call_id::text, logical_expires_at,
       purpose, status, provider, fleet_name, profile_snapshot, price_snapshot,
       context_snapshot, usage_sequence, reserved_nanos,
       issued_at IS NOT NULL AS issued, accounted_at IS NOT NULL AS accounted,
       known_usage, charged_nanos, canonical_receipt_digest,
       failure_receipt_digest, failure_reason,
       (EXTRACT(EPOCH FROM (clock_timestamp() - failure_recorded_at)) * 1000)::bigint AS failure_age_ms,
       GREATEST(0, FLOOR(EXTRACT(EPOCH FROM (logical_expires_at - clock_timestamp())) * 1000))::bigint AS remaining_ms,
       created_at
FROM runner.decision_calls
WHERE ($1::uuid IS NOT NULL AND id = $1::uuid)
   OR ($2::uuid IS NOT NULL AND original_lease_id = $2::uuid AND operation_id = $3::uuid)
   OR ($4::text IS NOT NULL AND assessment_key = $4 AND attempt_number = $5)
LIMIT 1
