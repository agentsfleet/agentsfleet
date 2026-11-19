SELECT id, draft_id
  FROM setup.attempts
 WHERE status = $1 AND LEAST(run_expires_at, expires_at) <= statement_timestamp()
 ORDER BY LEAST(run_expires_at, expires_at), attempts.id
 LIMIT $2
