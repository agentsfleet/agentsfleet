DELETE FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = $2
