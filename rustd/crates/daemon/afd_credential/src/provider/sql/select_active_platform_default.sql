SELECT d.provider,
       m.model_name,
       m.context_cap_tokens
  FROM model.platform_defaults d
  LEFT JOIN model.platform_models m ON m.id = d.platform_model_id
 WHERE d.active = true
 ORDER BY d.updated_at DESC, d.provider DESC
 LIMIT 1
