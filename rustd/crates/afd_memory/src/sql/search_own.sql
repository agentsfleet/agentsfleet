SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE fleet_id = $1::uuid
  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
ORDER BY (key ILIKE $2 ESCAPE '\') DESC, updated_at DESC, id DESC
LIMIT $3
