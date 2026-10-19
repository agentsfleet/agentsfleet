SELECT EXISTS(
    SELECT 1 FROM model.platform_defaults
    WHERE active AND platform_model_id IS NOT NULL
)
