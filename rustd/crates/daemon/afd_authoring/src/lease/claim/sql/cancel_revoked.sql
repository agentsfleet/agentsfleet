UPDATE authoring.messages m
   SET status = $5, finished_at = now(), updated_at = now()
  FROM authoring.drafts d
 WHERE m.draft_id = d.id AND d.id = $1
   AND d.workspace_id = $2 AND d.tenant_id = $3 AND d.user_id = $4
   AND m.status = $6
