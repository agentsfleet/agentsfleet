SELECT m.id::text, p.id::text AS price_id, m.display_name, m.context_cap_tokens,
       m.context_source_url, m.context_verified_at, p.input_nanos_per_mtok,
       p.cached_input_nanos_per_mtok, p.output_nanos_per_mtok
  FROM model.platform_models m
  JOIN model.platform_model_prices p ON p.platform_model_id = m.id
 WHERE m.id = $1::uuid
