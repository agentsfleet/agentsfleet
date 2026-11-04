SELECT EXISTS (
    SELECT 1 FROM runner.decision_calls
    WHERE original_lease_id = $1::uuid AND accounted_at IS NULL
)
