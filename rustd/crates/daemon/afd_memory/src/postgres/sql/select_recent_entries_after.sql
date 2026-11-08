SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND (created_at, key, fleet_id) < ($3, $4, $5::uuid)
ORDER BY created_at DESC, key DESC, fleet_id DESC LIMIT $6
