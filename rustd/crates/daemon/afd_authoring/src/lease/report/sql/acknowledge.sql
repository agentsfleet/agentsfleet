INSERT INTO billing.authoring_report_receipts
    (id, attempt_id, report_id, accounting_digest, acknowledgement, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5::jsonb, now())
