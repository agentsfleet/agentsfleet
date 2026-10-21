SELECT id::text FROM authoring.drafts WHERE tenant_id = $1::uuid FOR UPDATE
