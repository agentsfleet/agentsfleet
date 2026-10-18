UPDATE model.platform_models m
SET display_name = metadata.display_name,
    context_cap_tokens = metadata.context_cap_tokens,
    context_source_url = metadata.context_source_url,
    context_verified_at = metadata.context_verified_at,
    advanced = metadata.advanced,
    inference = COALESCE($3::jsonb, m.inference),
    updated_at = now()
FROM jsonb_to_record($2::jsonb) AS metadata(
    display_name text, context_cap_tokens integer, context_source_url text,
    context_verified_at timestamptz, advanced boolean)
WHERE m.id = $1::uuid
