INSERT INTO model.platform_models
  (id, provider, model_name, context_cap_tokens, input_nanos_per_mtok,
   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, now(), now())
ON CONFLICT (provider, model_name) DO NOTHING
