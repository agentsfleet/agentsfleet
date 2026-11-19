SELECT id,
       workspace_id,
       tenant_id,
       user_id
  FROM setup.drafts
 WHERE id = $1
