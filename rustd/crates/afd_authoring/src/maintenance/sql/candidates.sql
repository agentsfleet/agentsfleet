SELECT id::text AS id, tenant_id::text AS tenant_id, draft_id::text AS draft_id
  FROM authoring.attempts
 WHERE status = $1 AND LEAST(lease_expires_at, expires_at) <= statement_timestamp()
 ORDER BY LEAST(lease_expires_at, expires_at), id
 LIMIT $2
