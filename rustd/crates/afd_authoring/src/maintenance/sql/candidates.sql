SELECT id::text AS id, draft_id::text AS draft_id
  FROM authoring.attempts
 WHERE status = $1 AND LEAST(lease_expires_at, expires_at) <= statement_timestamp()
 ORDER BY LEAST(lease_expires_at, expires_at), attempts.id
 LIMIT $2
