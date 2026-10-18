SELECT m.model_name, m.provider, m.context_cap_tokens, p.input_nanos_per_mtok,
       p.cached_input_nanos_per_mtok, p.output_nanos_per_mtok,
       m.display_name, m.context_source_url, m.context_verified_at, m.advanced,
       m.updated_at, m.id::text,
       lower(normalize(model_name, NFKC)) AS display_key,
       lower(normalize(provider, NFKC)) AS vendor_key
FROM model.platform_models m
JOIN model.platform_model_prices p ON p.platform_model_id = m.id
 AND p.prompt_tokens_above = 0
 AND p.effective_from <= statement_timestamp()
 AND (p.effective_until IS NULL OR p.effective_until > statement_timestamp())
WHERE ($1::text IS NULL OR lower(normalize(provider, NFKC)) = lower(normalize($1, NFKC)))
  AND ($6::boolean OR NOT m.advanced)
  AND (lower(normalize(model_name, NFKC)) COLLATE "C",
       lower(normalize(provider, NFKC)) COLLATE "C", m.id)
      > ($2::text COLLATE "C", $3::text COLLATE "C", $4::uuid)
ORDER BY lower(normalize(model_name, NFKC)) COLLATE "C",
         lower(normalize(provider, NFKC)) COLLATE "C", m.id
LIMIT $5
