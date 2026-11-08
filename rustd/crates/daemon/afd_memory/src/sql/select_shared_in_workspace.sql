SELECT fleet_id, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE workspace_id = $1 AND workspace_visible AND fleet_id <> $2
ORDER BY updated_at DESC, id DESC
LIMIT $3
