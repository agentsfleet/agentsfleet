SELECT s.id
  FROM model.platform_defaults p
  JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
 WHERE p.active AND s.meta_has_key
   AND s.meta_provider = p.provider
 ORDER BY p.provider LIMIT 1
   FOR SHARE OF s
