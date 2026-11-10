DELETE FROM vault.secrets
 WHERE workspace_id = $1 AND key_name = $2
