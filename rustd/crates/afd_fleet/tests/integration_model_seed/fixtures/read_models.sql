SELECT m.model_name, m.context_cap_tokens,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok,
       p.output_nanos_per_mtok, m.inference
FROM model.platform_models m
JOIN model.platform_model_prices p ON p.platform_model_id = m.id
WHERE m.provider = $1
ORDER BY m.model_name
