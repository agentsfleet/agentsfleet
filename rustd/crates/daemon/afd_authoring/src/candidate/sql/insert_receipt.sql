INSERT INTO authoring.activity_receipts
    (id, attempt_id, sequence, request_digest, candidate_id, created_at)
VALUES ($1, $2, $3, $4, $5, now())
