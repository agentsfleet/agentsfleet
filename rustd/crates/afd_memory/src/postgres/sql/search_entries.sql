SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND (key ILIKE $3 ESCAPE '\' OR content ILIKE $3 ESCAPE '\')
ORDER BY created_at DESC, key DESC, fleet_id DESC
LIMIT $4
