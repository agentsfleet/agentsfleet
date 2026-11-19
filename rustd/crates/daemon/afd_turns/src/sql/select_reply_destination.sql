SELECT reply_provider, reply_address
FROM thread.turns
WHERE thread_id = $1 AND id = $2
  AND reply_provider IS NOT NULL
