SELECT m.provider, m.model_name, m.context_cap_tokens,
       jsonb_build_object('display_name', m.display_name, 'context_cap_tokens', m.context_cap_tokens,
           'context_source_url', m.context_source_url, 'context_verified_at', m.context_verified_at,
           'advanced', m.advanced) AS metadata,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.output_nanos_per_mtok
  FROM model.platform_models m
  JOIN model.platform_model_prices p ON p.platform_model_id = m.id
   AND p.prompt_tokens_above = 0
   AND p.effective_from <= statement_timestamp()
   AND (p.effective_until IS NULL OR p.effective_until > statement_timestamp())
 WHERE (provider, model_name) IN (SELECT p, m FROM unnest($1::text[], $2::text[]) AS u(p, m))
