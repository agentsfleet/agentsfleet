SELECT m.model_name, m.provider, m.context_cap_tokens, p.input_nanos_per_mtok,
       p.cached_input_nanos_per_mtok, p.output_nanos_per_mtok,
       jsonb_build_object('display_name', m.display_name, 'context_cap_tokens', m.context_cap_tokens,
           'context_source_url', m.context_source_url, 'context_verified_at', m.context_verified_at,
           'advanced', m.advanced) AS metadata,
       quotes.prices, greatest(m.updated_at, quotes.updated_at) AS updated_at, m.id::text,
       lower(normalize(model_name, NFKC)) AS display_key,
       lower(normalize(provider, NFKC)) AS vendor_key
FROM model.platform_models m
JOIN model.platform_model_prices p ON p.platform_model_id = m.id
 AND p.prompt_tokens_above = 0
 AND p.effective_from <= statement_timestamp()
 AND (p.effective_until IS NULL OR p.effective_until > statement_timestamp())
JOIN LATERAL (
    SELECT jsonb_agg(jsonb_build_object(
        'effective_from', q.effective_from, 'effective_until', q.effective_until,
        'prompt_tokens_above', q.prompt_tokens_above,
        'input_nanos_per_mtok', q.input_nanos_per_mtok,
        'cached_input_nanos_per_mtok', q.cached_input_nanos_per_mtok,
        'cache_write_nanos_per_mtok', q.cache_write_nanos_per_mtok,
        'cache_write_5m_nanos_per_mtok', q.cache_write_5m_nanos_per_mtok,
        'cache_write_1h_nanos_per_mtok', q.cache_write_1h_nanos_per_mtok,
        'output_nanos_per_mtok', q.output_nanos_per_mtok,
        'source_url', q.source_url, 'verified_at', q.verified_at, 'conditions', q.conditions
    ) ORDER BY q.prompt_tokens_above, q.effective_from) AS prices, max(q.updated_at) AS updated_at
    FROM model.platform_model_prices q WHERE q.platform_model_id = m.id
) quotes ON true
WHERE ($1::text IS NULL OR lower(normalize(provider, NFKC)) = lower(normalize($1, NFKC)))
  AND ($3::boolean OR NOT m.advanced)
ORDER BY lower(normalize(model_name, NFKC)) COLLATE "C",
         lower(normalize(provider, NFKC)) COLLATE "C", m.id
LIMIT $2
