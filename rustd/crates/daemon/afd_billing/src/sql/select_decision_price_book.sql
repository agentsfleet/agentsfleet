SELECT p.id AS id,
       p.effective_from_at AS effective_from, p.effective_until_at AS effective_until,
       p.prompt_tokens_above, p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok,
       p.cache_write_nanos_per_mtok, p.cache_write_5m_nanos_per_mtok,
       p.cache_write_1h_nanos_per_mtok, p.output_nanos_per_mtok,
       p.source_url, p.verified_at, p.conditions
FROM model.decision_prices p
WHERE p.platform_model_id = $1 AND p.decision_revision_id = $2
  AND p.effective_from_at <= $3::timestamptz
  AND (p.effective_until_at IS NULL OR p.effective_until_at > $3::timestamptz)
ORDER BY p.prompt_tokens_above
