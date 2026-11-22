UPDATE runner.checkpoint_purges p SET completed_at = clock_timestamp(), updated_at = now()
WHERE p.checkpoint_object_id = $1 AND p.workspace_id = $2 AND p.completed_at IS NULL
