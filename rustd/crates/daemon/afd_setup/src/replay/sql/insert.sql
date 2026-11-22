INSERT INTO thread.replay_receipts
    (scope, operation_id, request_digest, resource_id, created_at)
VALUES ($1, $2, $3, $4, now())
ON CONFLICT (scope, operation_id) DO NOTHING
