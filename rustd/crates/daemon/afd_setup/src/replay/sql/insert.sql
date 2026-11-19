INSERT INTO authoring.replay_receipts
    (id, workspace_id, user_id, scope, operation_id, request_digest, resource_id, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now())
ON CONFLICT (scope, operation_id) DO NOTHING
