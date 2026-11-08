INSERT INTO model.platform_defaults
  (provider, source_workspace_id, active, platform_model_id, base_url, created_at, updated_at)
SELECT $1, $2::uuid, true, m.id, $4, now(), now()
FROM model.platform_models m
WHERE m.provider = $1 AND m.model_name = $3
ON CONFLICT (provider) DO UPDATE
SET source_workspace_id = EXCLUDED.source_workspace_id,
    platform_model_id = EXCLUDED.platform_model_id,
    base_url = EXCLUDED.base_url,
    active = true,
    updated_at = EXCLUDED.updated_at
RETURNING updated_at
