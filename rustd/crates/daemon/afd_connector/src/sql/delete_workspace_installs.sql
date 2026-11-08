DELETE FROM connector.installs
WHERE provider = $1 AND workspace_id = $2::uuid
