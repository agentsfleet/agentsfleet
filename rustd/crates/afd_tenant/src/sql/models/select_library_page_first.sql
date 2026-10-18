SELECT model_name, provider, context_cap_tokens, input_nanos_per_mtok,
       cached_input_nanos_per_mtok, output_nanos_per_mtok, updated_at, id::text,
       lower(normalize(model_name, NFKC)) AS display_key,
       lower(normalize(provider, NFKC)) AS vendor_key
FROM model.platform_models
WHERE ($1::text IS NULL OR lower(normalize(provider, NFKC)) = lower(normalize($1, NFKC)))
ORDER BY lower(normalize(model_name, NFKC)) COLLATE "C",
         lower(normalize(provider, NFKC)) COLLATE "C", id
LIMIT $2
