SELECT id, title, status AS state, resource_version, generation,
       accepted_revision_id, created_at, expires_at
FROM fleet.tasks
WHERE fleet_id = $1 AND workspace_id = $2
  AND terminal_at IS NULL
  AND ($3::timestamptz IS NULL OR (created_at, id) < ($3::timestamptz, $4))
ORDER BY created_at DESC, id DESC
LIMIT $5
