-- The lease's next decision call number. Decision lines are their own
-- `kind`, so they count from 0 and share no numbers with model or runtime lines.
SELECT COALESCE(MAX(usage_sequence) + 1, 0)::bigint
FROM runner.decision_calls WHERE original_lease_id = $1
