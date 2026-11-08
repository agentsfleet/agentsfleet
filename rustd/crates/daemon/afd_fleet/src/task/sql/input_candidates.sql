SELECT id::text, title, status AS state, generation
FROM fleet.tasks
WHERE fleet_id = $1::uuid AND workspace_id = $2::uuid AND terminal_at IS NULL
ORDER BY created_at DESC, id DESC
LIMIT $3
