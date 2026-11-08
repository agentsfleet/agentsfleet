INSERT INTO billing.authoring_report_receipts
    (id, attempt_id, report_id, accounting_digest, acknowledgement, created_at)
VALUES ($1, $2, $3, $4, $5::jsonb, now())
