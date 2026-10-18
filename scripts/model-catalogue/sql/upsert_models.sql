INSERT INTO model.platform_models (
    id, provider, model_name, display_name, advanced, context_cap_tokens,
    context_source_url, context_verified_at, created_at, updated_at
)
SELECT id, provider, model_name, display_name, advanced, context_cap_tokens,
       context_source_url, context_verified_at, now(), now()
FROM jsonb_to_recordset($1::jsonb) AS wanted(
    id uuid, provider text, model_name text, display_name text, advanced boolean,
    context_cap_tokens integer, context_source_url text, context_verified_at timestamptz
)
ON CONFLICT (provider, model_name) DO UPDATE SET
    display_name = EXCLUDED.display_name,
    advanced = EXCLUDED.advanced,
    context_cap_tokens = EXCLUDED.context_cap_tokens,
    context_source_url = EXCLUDED.context_source_url,
    context_verified_at = EXCLUDED.context_verified_at,
    updated_at = now()
WHERE (platform_models.display_name, platform_models.advanced,
       platform_models.context_cap_tokens, platform_models.context_source_url,
       platform_models.context_verified_at)
    IS DISTINCT FROM
      (EXCLUDED.display_name, EXCLUDED.advanced, EXCLUDED.context_cap_tokens,
       EXCLUDED.context_source_url, EXCLUDED.context_verified_at);
