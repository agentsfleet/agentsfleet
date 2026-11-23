SELECT id, payload_digest AS digest, thread_id AS fleet, created_at
FROM thread.turns
WHERE producer = $1 AND producer_key = $2
