UPDATE authoring.messages m
   SET status = $5, finished_at = now(), updated_at = now()
  FROM authoring.drafts d
 WHERE m.draft_id = d.id AND d.id = $1::uuid
   AND d.workspace_id = $2::uuid AND d.tenant_id = $3::uuid AND d.user_id = $4::uuid
   AND m.status = $6
