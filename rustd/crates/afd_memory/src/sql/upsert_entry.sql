INSERT INTO memory.entries
  (id, key, content, category, fleet_id, workspace_id, workspace_visible, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6::uuid, $7, $8, $8)
ON CONFLICT (fleet_id, key) DO UPDATE
  SET content = EXCLUDED.content,
      category = EXCLUDED.category,
      workspace_visible = EXCLUDED.workspace_visible,
      updated_at = EXCLUDED.updated_at
