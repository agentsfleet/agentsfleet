SELECT p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.output_nanos_per_mtok
FROM model.platform_models m
JOIN model.platform_model_prices p ON p.platform_model_id = m.id
 AND p.prompt_tokens_above = 0
 AND p.effective_from <= statement_timestamp()
 AND (p.effective_until IS NULL OR p.effective_until > statement_timestamp())
WHERE m.provider = $1 AND m.model_name = $2
