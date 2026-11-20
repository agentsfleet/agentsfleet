SELECT sequence, tenant_model_id
  FROM thread.turns
 WHERE thread_id = $1 AND status = $2 AND settled_at IS NULL
 ORDER BY sequence
 LIMIT 1
