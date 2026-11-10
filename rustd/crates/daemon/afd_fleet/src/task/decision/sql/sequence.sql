SELECT COALESCE(MAX(usage_sequence) + 1, $2)::bigint
FROM runner.decision_calls WHERE original_lease_id = $1
