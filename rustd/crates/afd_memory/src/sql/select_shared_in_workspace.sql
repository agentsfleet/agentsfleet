SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE workspace_id = $1::uuid AND workspace_visible AND fleet_id <> $2::uuid
ORDER BY updated_at DESC, id DESC
LIMIT $3
