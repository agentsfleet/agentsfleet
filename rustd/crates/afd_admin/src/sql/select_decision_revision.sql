UPDATE model.platform_models m
SET decision_revision_id = d.id, decision_enabled = $3::boolean, updated_at = now()
FROM model.decision_revisions d
WHERE m.id = $1::uuid AND d.id = $2::uuid AND d.platform_model_id = m.id
