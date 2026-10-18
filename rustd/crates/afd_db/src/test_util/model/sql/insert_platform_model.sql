INSERT INTO model.platform_models
  (id, model_name, provider, context_cap_tokens, input_nanos_per_mtok,
   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5, $6, $7,
        coalesce($8::timestamptz, now()), coalesce($8::timestamptz, now()))
RETURNING id::text
