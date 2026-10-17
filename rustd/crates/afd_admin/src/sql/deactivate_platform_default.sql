UPDATE model.platform_defaults
SET active = false, platform_model_id = NULL, updated_at = now()
WHERE provider = $1
