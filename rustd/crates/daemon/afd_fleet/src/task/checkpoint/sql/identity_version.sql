SELECT COALESCE((SELECT key_version FROM fleet.task_identity_keys
    WHERE workspace_id = $1 AND repository = $2
    ORDER BY key_version DESC LIMIT 1), 0) + 1
