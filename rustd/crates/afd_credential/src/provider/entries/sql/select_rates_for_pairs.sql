SELECT provider, model_name, context_cap_tokens,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok
  FROM model.platform_models
 WHERE (provider, model_name) IN (SELECT p, m FROM unnest($1::text[], $2::text[]) AS u(p, m))
