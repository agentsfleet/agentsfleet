SELECT id::text, title, status AS state, resource_version, generation,
       accepted_revision_id::text, created_at, expires_at
FROM fleet.tasks
WHERE fleet_id = $1::uuid AND workspace_id = $2::uuid
  AND terminal_at IS NULL
  AND ($3::timestamptz IS NULL OR (created_at, id) < ($3::timestamptz, $4::uuid))
ORDER BY created_at DESC, id DESC
LIMIT $5
