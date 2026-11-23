SELECT sequence
  FROM thread.turns
 WHERE thread_id = $1 AND status = $2
 ORDER BY sequence
 LIMIT 1
