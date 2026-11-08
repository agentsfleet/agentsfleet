SELECT workspace_id::text
FROM connector.installs
WHERE provider = $1 AND external_account_id = $2
