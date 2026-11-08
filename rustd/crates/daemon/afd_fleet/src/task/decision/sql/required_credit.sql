SELECT COALESCE(SUM(reserved_nanos), 0)::bigint
FROM runner.decision_calls
WHERE original_lease_id = $1::uuid AND accounted_at IS NULL
