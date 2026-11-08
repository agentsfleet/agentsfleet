SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE fleet_id = $1::uuid
ORDER BY updated_at DESC, id DESC
