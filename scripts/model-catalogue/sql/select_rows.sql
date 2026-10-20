SELECT m.provider, m.model_name, m.display_name, m.advanced, m.inference,
       m.context_cap_tokens, m.context_source_url,
       to_jsonb(m.context_verified_at) #>> '{}' AS context_verified_at,
       COALESCE(jsonb_agg(jsonb_build_object(
           'effective_from', p.effective_from,
           'effective_until', p.effective_until,
           'prompt_tokens_above', p.prompt_tokens_above,
           'input_nanos_per_mtok', p.input_nanos_per_mtok::text,
           'cached_input_nanos_per_mtok', p.cached_input_nanos_per_mtok::text,
           'cache_write_nanos_per_mtok', p.cache_write_nanos_per_mtok::text,
           'cache_write_5m_nanos_per_mtok', p.cache_write_5m_nanos_per_mtok::text,
           'cache_write_1h_nanos_per_mtok', p.cache_write_1h_nanos_per_mtok::text,
           'output_nanos_per_mtok', p.output_nanos_per_mtok::text,
           'source_url', p.source_url,
           'verified_at', p.verified_at,
           'conditions', p.conditions
       )) FILTER (WHERE p.id IS NOT NULL), '[]'::jsonb) AS prices
FROM model.platform_models AS m
LEFT JOIN model.platform_model_prices AS p ON p.platform_model_id = m.id
GROUP BY m.id
ORDER BY m.provider, m.model_name;
