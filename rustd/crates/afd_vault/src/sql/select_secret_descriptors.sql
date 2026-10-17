SELECT key_name, meta_kind, meta_provider, meta_base_url, meta_has_key
  FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = ANY($2::text[])
