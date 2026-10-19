SELECT d.provider, d.source_workspace_id::text, m.model_name, d.active, d.updated_at
FROM model.platform_defaults d
LEFT JOIN model.platform_models m ON m.id = d.platform_model_id
ORDER BY d.provider
