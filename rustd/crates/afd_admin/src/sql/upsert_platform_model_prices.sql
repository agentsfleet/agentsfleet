INSERT INTO model.platform_model_prices
    (id, platform_model_id, effective_from, effective_until, prompt_tokens_above,
     input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok,
     cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
     source_url, verified_at, conditions, created_at, updated_at)
SELECT uuidv7(), $1::uuid, p.effective_from, p.effective_until, p.prompt_tokens_above,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.cache_write_nanos_per_mtok,
       p.cache_write_5m_nanos_per_mtok, p.cache_write_1h_nanos_per_mtok, p.output_nanos_per_mtok,
       p.source_url, p.verified_at, p.conditions, now(), now()
FROM jsonb_to_recordset($2::jsonb) AS p(
    effective_from timestamptz, effective_until timestamptz, prompt_tokens_above integer,
    input_nanos_per_mtok bigint, cached_input_nanos_per_mtok bigint, cache_write_nanos_per_mtok bigint,
    cache_write_5m_nanos_per_mtok bigint, cache_write_1h_nanos_per_mtok bigint, output_nanos_per_mtok bigint,
    source_url text, verified_at timestamptz, conditions text)
ON CONFLICT (platform_model_id, prompt_tokens_above, effective_from) DO UPDATE SET
    effective_until = EXCLUDED.effective_until,
    input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
    cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
    cache_write_nanos_per_mtok = EXCLUDED.cache_write_nanos_per_mtok,
    cache_write_5m_nanos_per_mtok = EXCLUDED.cache_write_5m_nanos_per_mtok,
    cache_write_1h_nanos_per_mtok = EXCLUDED.cache_write_1h_nanos_per_mtok,
    output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
    source_url = EXCLUDED.source_url, verified_at = EXCLUDED.verified_at,
    conditions = EXCLUDED.conditions, updated_at = now()
