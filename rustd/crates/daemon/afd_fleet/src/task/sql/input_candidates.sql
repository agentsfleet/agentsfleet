SELECT id, title, status AS state, generation
FROM fleet.tasks
WHERE fleet_id = $1 AND workspace_id = $2 AND terminal_at IS NULL
ORDER BY created_at DESC, id DESC
LIMIT $3
