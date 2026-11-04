SELECT (
    COALESCE((SELECT SUM(charged_nanos) FROM billing.usage_lines
              WHERE work_type = $1 AND work_id = $2::uuid), 0)
    + COALESCE((SELECT SUM(reserved_nanos) FROM runner.decision_calls
                WHERE original_lease_id = $2::uuid AND accounted_at IS NULL), 0)
)::bigint
