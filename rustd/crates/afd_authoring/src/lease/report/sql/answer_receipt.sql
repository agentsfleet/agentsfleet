INSERT INTO authoring.answer_receipts
    (id, draft_id, attempt_id, report_id, answer_digest, expires_at, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, clock_timestamp() + $6::interval, now())
