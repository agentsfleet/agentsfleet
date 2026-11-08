SELECT id, original_runner_id, original_lease_id,
       source_admission_id, fleet_id, workspace_id, tenant_id,
       task_id, fencing_token, request_digest, assessment_id,
       assessment_key, attempt_number, retry_of_decision_call_id, logical_expires_at,
       purpose, status, provider, fleet_name, profile_snapshot, price_snapshot,
       context_snapshot, usage_sequence, reserved_nanos,
       issued_at IS NOT NULL AS issued, accounted_at IS NOT NULL AS accounted,
       known_usage, charged_nanos, canonical_receipt_digest,
       failure_receipt_digest, failure_reason,
       (EXTRACT(EPOCH FROM (clock_timestamp() - failure_recorded_at)) * 1000)::bigint AS failure_age_ms,
       GREATEST(0, FLOOR(EXTRACT(EPOCH FROM (logical_expires_at - clock_timestamp())) * 1000))::bigint AS remaining_ms,
       created_at
FROM runner.decision_calls
WHERE (id = $1 AND $1 IS NOT NULL)
   OR (original_lease_id = $2 AND operation_id = $3 AND $2 IS NOT NULL)
   OR ($4::text IS NOT NULL AND assessment_key = $4 AND attempt_number = $5)
LIMIT 1
