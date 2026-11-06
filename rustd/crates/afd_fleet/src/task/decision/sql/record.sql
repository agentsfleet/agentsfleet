WITH instant AS MATERIALIZED (SELECT date_trunc('milliseconds', clock_timestamp()) AS at)
UPDATE runner.decision_calls c
SET status = $2,
    accounted_at = CASE WHEN $3 THEN COALESCE(c.accounted_at, instant.at) ELSE c.accounted_at END,
    known_usage = COALESCE(c.known_usage, $4::jsonb),
    charged_nanos = COALESCE(c.charged_nanos, $5),
    canonical_receipt_digest = COALESCE(c.canonical_receipt_digest, $6),
    failure_receipt_digest = COALESCE(c.failure_receipt_digest, $7),
    failure_reason = COALESCE(c.failure_reason, $8::jsonb),
    failure_recorded_at = CASE WHEN c.failure_receipt_digest IS NULL AND $7::text IS NOT NULL
                              THEN instant.at ELSE c.failure_recorded_at END,
    reported_model = COALESCE(c.reported_model, $9), updated_at = instant.at
FROM instant WHERE c.id = $1::uuid
RETURNING c.updated_at AS at
