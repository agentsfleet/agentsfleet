UPDATE thread.turns m
   SET status = $5, settled_at = now(), updated_at = now()
  FROM setup.drafts d
 WHERE m.thread_id = d.id AND d.id = $1
   AND d.workspace_id = $2 AND d.tenant_id = $3 AND d.user_id = $4
   AND m.status = $6
