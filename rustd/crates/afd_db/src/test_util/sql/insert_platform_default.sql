WITH model AS (
    INSERT INTO model.platform_models
        (id, model_name, provider, display_name, context_cap_tokens,
         context_source_url, context_verified_at, advanced, inference, created_at, updated_at)
    VALUES ($6::uuid, $3, $1, $3, $4,
            'https://fixtures.agentsfleet.net/models', $5, false, $7::jsonb, $5, $5)
    ON CONFLICT (provider, model_name) DO UPDATE SET model_name = EXCLUDED.model_name
    RETURNING id
), price AS (
    INSERT INTO model.platform_model_prices
        (id, platform_model_id, effective_from, effective_until, prompt_tokens_above,
         input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
         source_url, verified_at, conditions, created_at, updated_at)
    SELECT $6::uuid, m.id, $5, NULL, 0, 0, 0, 0,
           'https://fixtures.agentsfleet.net/pricing', $5,
           'Zero-priced test fixture; not a provider quotation.', now(), now()
      FROM model m
     WHERE NOT EXISTS (
           SELECT 1 FROM model.platform_model_prices p WHERE p.platform_model_id = m.id
     )
)
INSERT INTO model.platform_defaults (provider, source_workspace_id, active, platform_model_id, created_at, updated_at)
SELECT $1, $2::uuid, TRUE, id, $5, $5 FROM model
ON CONFLICT (provider) DO NOTHING
RETURNING provider
