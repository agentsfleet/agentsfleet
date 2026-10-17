SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE workspace_id = $1::uuid
ORDER BY fleet_id, key
