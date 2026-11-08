SELECT id::text AS id,
       workspace_id::text AS workspace_id,
       tenant_id::text AS tenant_id,
       user_id::text AS user_id
  FROM authoring.drafts
 WHERE id = $1::uuid
