INSERT INTO model.platform_model_prices
    (id, platform_model_id, effective_from, effective_until, prompt_tokens_above,
     input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok,
     cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
     source_url, verified_at, conditions, created_at, updated_at)
SELECT p.id, $1::uuid, p.effective_from, p.effective_until, p.prompt_tokens_above,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.cache_write_nanos_per_mtok,
       p.cache_write_5m_nanos_per_mtok, p.cache_write_1h_nanos_per_mtok, p.output_nanos_per_mtok,
       p.source_url, p.verified_at, p.conditions, now(), now()
FROM unnest($2::text[]::uuid[], $3::timestamptz[], $4::timestamptz[], $5::integer[],
            $6::bigint[], $7::bigint[], $8::bigint[], $9::bigint[], $10::bigint[], $11::bigint[],
            $12::text[], $13::timestamptz[], $14::text[])
    AS p(id, effective_from, effective_until, prompt_tokens_above,
         input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok,
         cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
         source_url, verified_at, conditions)
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
