INSERT INTO model.decision_revisions (id, platform_model_id, revision, definition, created_at)
VALUES ($1, $2, $3, $4::jsonb, now())
ON CONFLICT (platform_model_id, revision) DO NOTHING
