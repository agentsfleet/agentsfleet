SELECT m.provider
FROM model.platform_models m
JOIN model.decision_revisions r ON r.platform_model_id = m.id AND r.id = $2
WHERE m.id = $1 AND m.decision_enabled
