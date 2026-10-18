SELECT platform_model_id::text, effective_from, effective_until, prompt_tokens_above,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok,
       cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, output_nanos_per_mtok,
       source_url, verified_at, conditions
FROM model.platform_model_prices
WHERE platform_model_id = ANY($1::text[]::uuid[])
ORDER BY platform_model_id, prompt_tokens_above, effective_from
