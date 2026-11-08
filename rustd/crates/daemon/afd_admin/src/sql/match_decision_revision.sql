SELECT id::text, definition = $3::jsonb AS identical
FROM model.decision_revisions
WHERE platform_model_id = $1::uuid AND revision = $2
