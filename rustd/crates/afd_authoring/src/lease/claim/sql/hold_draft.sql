SELECT id::text FROM authoring.drafts WHERE id = $1::uuid FOR UPDATE NOWAIT
