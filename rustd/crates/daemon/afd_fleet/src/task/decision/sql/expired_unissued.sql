SELECT c.id::text, c.logical_expires_at AS expires_at,
       COALESCE($4::timestamptz, statement_timestamp()) AS cycle_cutoff,
       GREATEST(0, FLOOR(EXTRACT(EPOCH FROM (statement_timestamp() - c.logical_expires_at)) * 1000))::bigint AS overdue_ms
FROM runner.decision_calls c
WHERE c.issued_at IS NULL AND c.accounted_at IS NULL
  AND c.logical_expires_at <= LEAST(COALESCE($4::timestamptz, statement_timestamp()), statement_timestamp())
  AND (c.logical_expires_at, c.id) > (COALESCE($2::timestamptz, '-infinity'::timestamptz), $3::uuid)
ORDER BY c.logical_expires_at, c.id
LIMIT $1
