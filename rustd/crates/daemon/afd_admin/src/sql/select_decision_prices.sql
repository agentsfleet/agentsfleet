SELECT p.platform_model_id::text, p.effective_from_at AS effective_from,
       p.effective_until_at AS effective_until, p.prompt_tokens_above,
       p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, p.cache_write_nanos_per_mtok,
       p.cache_write_5m_nanos_per_mtok, p.cache_write_1h_nanos_per_mtok, p.output_nanos_per_mtok,
       p.source_url, p.verified_at, p.conditions
FROM model.decision_prices p
JOIN model.platform_models m ON m.id = p.platform_model_id AND m.decision_revision_id = p.decision_revision_id
WHERE p.platform_model_id = ANY($1::text[]::uuid[])
ORDER BY p.platform_model_id, p.prompt_tokens_above, p.effective_from_at
