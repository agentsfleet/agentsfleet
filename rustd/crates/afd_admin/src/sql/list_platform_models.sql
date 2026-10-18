SELECT id::text, provider, model_name, context_cap_tokens,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, inference
FROM model.platform_models
ORDER BY provider, model_name
