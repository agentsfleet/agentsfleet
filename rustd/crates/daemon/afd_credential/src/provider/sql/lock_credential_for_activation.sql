SELECT s.meta_provider, s.meta_has_key,
       s.dek_ciphertext, s.dek_nonce, s.dek_tag,
       s.secret_nonce, s.secret_ciphertext, s.secret_tag, s.kek_version,
       w.id::text AS workspace_id
  FROM vault.secrets s
  JOIN (SELECT id FROM identity.workspaces
         WHERE tenant_id = $1::uuid
         ORDER BY created_at ASC, id ASC
         LIMIT 1) w ON s.workspace_id = w.id
 WHERE s.key_name = $2
   FOR UPDATE OF s
