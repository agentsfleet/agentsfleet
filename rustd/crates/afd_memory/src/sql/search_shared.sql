SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE workspace_id = $1::uuid AND workspace_visible AND fleet_id <> $2::uuid
  AND (key ILIKE $3 ESCAPE '\' OR content ILIKE $3 ESCAPE '\')
ORDER BY (key ILIKE $3 ESCAPE '\') DESC, updated_at DESC, id DESC
LIMIT $4
