SELECT key_name, created_at, meta_kind, meta_provider, meta_base_url
  FROM vault.secrets
 WHERE workspace_id = $1::uuid
 ORDER BY key_name ASC
