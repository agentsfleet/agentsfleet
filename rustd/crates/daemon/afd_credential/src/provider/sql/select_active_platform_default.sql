SELECT d.provider,
       d.source_workspace_id::text AS source_workspace_id,
       m.model_name,
       d.base_url,
       m.context_cap_tokens
  FROM model.platform_defaults d
  LEFT JOIN model.platform_models m ON m.id = d.platform_model_id
 WHERE d.active = true
 ORDER BY d.updated_at DESC, d.provider DESC
 LIMIT 1
