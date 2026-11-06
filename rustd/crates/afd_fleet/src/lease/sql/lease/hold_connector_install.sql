SELECT external_account_id FROM connector.installs
WHERE provider = $1 AND external_account_id = $2 AND workspace_id = $3::uuid
FOR SHARE
