INSERT INTO model.platform_models
    (id, provider, model_name, display_name, context_cap_tokens,
     context_source_url, context_verified_at, advanced, inference, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, now(), now())
ON CONFLICT (provider, model_name) DO NOTHING
