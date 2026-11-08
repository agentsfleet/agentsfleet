INSERT INTO authoring.replay_receipts
    (id, workspace_id, user_id, scope, operation_id, request_digest, resource_id, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5::uuid, $6, $7::uuid, now())
ON CONFLICT (scope, operation_id) DO NOTHING
