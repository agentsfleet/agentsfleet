SELECT s.meta_provider, s.meta_has_key,
       s.dek_ciphertext, s.dek_nonce, s.dek_tag,
       s.secret_nonce, s.secret_ciphertext, s.secret_tag, s.kek_version,
       w.id AS workspace_id
  FROM vault.secrets s
  JOIN identity.workspaces w ON w.id = s.workspace_id AND w.tenant_id = $1
 WHERE s.key_name = $2
   AND s.workspace_id = $3
   FOR UPDATE OF s
