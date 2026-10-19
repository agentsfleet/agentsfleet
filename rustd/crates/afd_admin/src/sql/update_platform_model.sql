UPDATE model.platform_models
SET display_name = $2,
    context_cap_tokens = $3,
    context_source_url = $4,
    context_verified_at = $5,
    advanced = $6,
    inference = COALESCE($7::jsonb, inference),
    updated_at = now()
WHERE id = $1::uuid
