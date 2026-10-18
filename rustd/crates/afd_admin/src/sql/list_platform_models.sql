SELECT m.id::text, m.provider, m.model_name,
       jsonb_build_object('display_name', m.display_name, 'context_cap_tokens', m.context_cap_tokens,
           'context_source_url', m.context_source_url, 'context_verified_at', m.context_verified_at,
           'advanced', m.advanced) AS metadata,
       m.inference,
       COALESCE(jsonb_agg(jsonb_build_object(
           'effective_from', p.effective_from, 'effective_until', p.effective_until,
           'prompt_tokens_above', p.prompt_tokens_above,
           'input_nanos_per_mtok', p.input_nanos_per_mtok,
           'cached_input_nanos_per_mtok', p.cached_input_nanos_per_mtok,
           'cache_write_nanos_per_mtok', p.cache_write_nanos_per_mtok,
           'cache_write_5m_nanos_per_mtok', p.cache_write_5m_nanos_per_mtok,
           'cache_write_1h_nanos_per_mtok', p.cache_write_1h_nanos_per_mtok,
           'output_nanos_per_mtok', p.output_nanos_per_mtok,
           'source_url', p.source_url, 'verified_at', p.verified_at, 'conditions', p.conditions
       ) ORDER BY p.prompt_tokens_above, p.effective_from) FILTER (WHERE p.id IS NOT NULL), '[]'::jsonb) AS prices
FROM model.platform_models m
LEFT JOIN model.platform_model_prices p ON p.platform_model_id = m.id
GROUP BY m.id
ORDER BY m.provider, m.model_name
