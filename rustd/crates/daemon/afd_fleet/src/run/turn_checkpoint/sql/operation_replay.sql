SELECT resource_id, request_digest
FROM thread.replay_receipts
WHERE scope = $1 AND operation_id = $2
