SELECT id::text, provider, model_name, display_name, context_cap_tokens,
       context_source_url, context_verified_at, advanced, inference
FROM model.platform_models
ORDER BY provider, model_name
