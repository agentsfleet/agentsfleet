INSERT INTO billing.authoring_settlements
    (id, attempt_id, report_id, accounting_digest, outcome, failure_reason, usage,
     charged_nanos, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7::jsonb, $8, now())
