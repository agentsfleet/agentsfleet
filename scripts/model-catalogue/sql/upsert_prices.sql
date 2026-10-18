INSERT INTO model.platform_model_prices (
    id, platform_model_id, effective_from, effective_until, prompt_tokens_above,
    input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok, cache_write_5m_nanos_per_mtok,
    cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
    source_url, verified_at, conditions, created_at, updated_at
)
SELECT wanted.id, m.id, effective_from, effective_until, prompt_tokens_above,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok, cache_write_5m_nanos_per_mtok,
       cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
       source_url, verified_at, conditions, now(), now()
FROM jsonb_to_recordset($1::jsonb) AS wanted(
    id uuid, provider text, model_name text,
    effective_from timestamptz, effective_until timestamptz, prompt_tokens_above integer,
    input_nanos_per_mtok bigint, cached_input_nanos_per_mtok bigint,
    cache_write_nanos_per_mtok bigint,
    cache_write_5m_nanos_per_mtok bigint, cache_write_1h_nanos_per_mtok bigint,
    output_nanos_per_mtok bigint, source_url text, verified_at timestamptz, conditions text
)
JOIN model.platform_models AS m USING (provider, model_name)
ON CONFLICT (platform_model_id, prompt_tokens_above, effective_from) DO UPDATE SET
    effective_until = EXCLUDED.effective_until,
    input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
    cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
    cache_write_nanos_per_mtok = EXCLUDED.cache_write_nanos_per_mtok,
    cache_write_5m_nanos_per_mtok = EXCLUDED.cache_write_5m_nanos_per_mtok,
    cache_write_1h_nanos_per_mtok = EXCLUDED.cache_write_1h_nanos_per_mtok,
    output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
    source_url = EXCLUDED.source_url,
    verified_at = EXCLUDED.verified_at,
    conditions = EXCLUDED.conditions,
    updated_at = now()
WHERE (platform_model_prices.effective_until, platform_model_prices.input_nanos_per_mtok,
       platform_model_prices.cached_input_nanos_per_mtok, platform_model_prices.cache_write_nanos_per_mtok,
       platform_model_prices.cache_write_5m_nanos_per_mtok,
       platform_model_prices.cache_write_1h_nanos_per_mtok, platform_model_prices.output_nanos_per_mtok,
       platform_model_prices.source_url, platform_model_prices.verified_at, platform_model_prices.conditions)
    IS DISTINCT FROM
      (EXCLUDED.effective_until, EXCLUDED.input_nanos_per_mtok, EXCLUDED.cached_input_nanos_per_mtok,
       EXCLUDED.cache_write_nanos_per_mtok,
       EXCLUDED.cache_write_5m_nanos_per_mtok, EXCLUDED.cache_write_1h_nanos_per_mtok,
       EXCLUDED.output_nanos_per_mtok, EXCLUDED.source_url, EXCLUDED.verified_at, EXCLUDED.conditions);
