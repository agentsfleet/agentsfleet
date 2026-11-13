SELECT s.workspace_id, s.key_name, s.meta_kind, s.meta_provider,
       s.meta_base_url, s.meta_has_key
  FROM unnest($1::uuid[], $2::text[]) AS r(workspace_id, key_name)
  JOIN vault.secrets s ON s.workspace_id = r.workspace_id AND s.key_name = r.key_name
