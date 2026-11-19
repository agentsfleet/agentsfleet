SELECT id, workspace_id, repository, created_at
FROM fleet.task_identity_keys
WHERE created_at < statement_timestamp()-make_interval(secs=>$1)
  AND ($2::timestamptz IS NULL OR (created_at,id)>($2,$3::uuid))
ORDER BY created_at,id LIMIT $4
