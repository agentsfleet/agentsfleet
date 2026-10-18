WITH model AS (
    INSERT INTO model.platform_models (id, model_name, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, inference, created_at, updated_at)
    VALUES ($6::uuid, $3, $1, $4, 0, 0, 0, $7::jsonb, $5, $5)
    ON CONFLICT (provider, model_name) DO UPDATE SET model_name = EXCLUDED.model_name
    RETURNING id
)
INSERT INTO model.platform_defaults (provider, source_workspace_id, active, platform_model_id, created_at, updated_at)
SELECT $1, $2::uuid, TRUE, id, $5, $5 FROM model
ON CONFLICT (provider) DO NOTHING
RETURNING provider
