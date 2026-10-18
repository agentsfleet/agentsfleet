WITH identity AS (
    INSERT INTO model.platform_models
        (id, provider, model_name, display_name, context_cap_tokens,
         context_source_url, context_verified_at, advanced, created_at, updated_at)
    VALUES ($1::uuid, $2, $3, $3, $4, 'https://fixtures.agentsfleet.net/models',
            COALESCE($8::timestamptz, now()), false, now(), now())
    ON CONFLICT (provider, model_name) DO UPDATE
        SET context_cap_tokens = EXCLUDED.context_cap_tokens,
            context_verified_at = EXCLUDED.context_verified_at, updated_at = now()
    RETURNING id
)
INSERT INTO model.platform_model_prices
    (id, platform_model_id, effective_from, prompt_tokens_above,
     input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
     source_url, verified_at, conditions, created_at, updated_at)
SELECT uuidv7(), m.id, to_timestamp(0), 0, $5, $6, $7,
       'https://fixtures.agentsfleet.net/pricing', COALESCE($8::timestamptz, now()),
       'Synthetic test fixture; not a provider quotation.', now(), now()
  FROM identity m
ON CONFLICT (platform_model_id, prompt_tokens_above, effective_from) DO UPDATE
    SET input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
        output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
        updated_at = now()
