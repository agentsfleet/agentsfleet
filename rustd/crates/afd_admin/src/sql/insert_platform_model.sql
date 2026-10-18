INSERT INTO model.platform_models
    (id, provider, model_name, display_name, context_cap_tokens,
     context_source_url, context_verified_at, advanced, inference, created_at, updated_at)
SELECT $1::uuid, $2, $3, display_name, context_cap_tokens,
       context_source_url, context_verified_at, advanced, $5::jsonb, now(), now()
FROM jsonb_to_record($4::jsonb) AS metadata(
    display_name text, context_cap_tokens integer, context_source_url text,
    context_verified_at timestamptz, advanced boolean)
ON CONFLICT (provider, model_name) DO NOTHING
