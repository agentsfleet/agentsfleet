SELECT fleet_id, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE (fleet_id = $1 OR (workspace_id = $2 AND workspace_visible))
  AND category = $3
  AND (created_at, key, fleet_id) < ($4, $5, $6)
ORDER BY created_at DESC, key DESC, fleet_id DESC LIMIT $7
