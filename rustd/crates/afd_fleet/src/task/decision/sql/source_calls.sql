SELECT COUNT(*)::bigint FROM runner.decision_calls
WHERE source_admission_id = $1::uuid AND purpose = $2
