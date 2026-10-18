SELECT user_id::text AS user_id, request_digest, resource_id::text AS resource_id
  FROM authoring.replay_receipts
 WHERE scope = $1 AND operation_id = $2::uuid
