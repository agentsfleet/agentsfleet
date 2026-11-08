SELECT id,
       workspace_id,
       tenant_id,
       user_id
  FROM authoring.drafts
 WHERE id = $1
