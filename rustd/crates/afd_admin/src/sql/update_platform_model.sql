UPDATE model.platform_models
SET context_cap_tokens = $2,
    input_nanos_per_mtok = $3,
    cached_input_nanos_per_mtok = $4,
    output_nanos_per_mtok = $5,
    updated_at = now()
WHERE id = $1::uuid
