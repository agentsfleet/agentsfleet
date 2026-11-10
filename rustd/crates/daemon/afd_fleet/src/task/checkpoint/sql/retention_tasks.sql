SELECT id, fleet_id, workspace_id, checkpoint_cleanup_at FROM fleet.tasks
WHERE checkpoint_cleanup_at IS NOT NULL AND checkpoint_cleanup_at <= $1::timestamptz
  AND ($2::timestamptz IS NULL OR (checkpoint_cleanup_at, id) > ($2::timestamptz, $3))
ORDER BY checkpoint_cleanup_at, id LIMIT $4
