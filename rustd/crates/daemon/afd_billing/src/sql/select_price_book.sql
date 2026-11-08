SELECT p.id::text AS id, p.effective_from, p.effective_until, p.prompt_tokens_above,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.cache_write_nanos_per_mtok,
       p.cache_write_5m_nanos_per_mtok, p.cache_write_1h_nanos_per_mtok,
       p.output_nanos_per_mtok, p.source_url, p.verified_at, p.conditions
FROM model.platform_models m
JOIN model.platform_model_prices p ON p.platform_model_id = m.id
WHERE m.provider = $1 AND m.model_name = $2
ORDER BY p.effective_from, p.prompt_tokens_above
