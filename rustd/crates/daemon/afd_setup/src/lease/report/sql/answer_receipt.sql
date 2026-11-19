INSERT INTO authoring.answer_receipts
    (id, draft_id, attempt_id, report_id, answer_digest, expires_at, created_at)
VALUES ($1, $2, $3, $4, $5, clock_timestamp() + $6::interval, now())
