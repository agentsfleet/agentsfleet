INSERT INTO runner.turn_checkpoint_refs (turn_checkpoint_id, workspace_id, task_object_id, created_at)
SELECT $1, $2, unnest($3::uuid[]), now()
ON CONFLICT DO NOTHING
