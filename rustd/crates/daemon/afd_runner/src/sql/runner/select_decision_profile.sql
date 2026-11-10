SELECT m.id AS model_id, m.model_name, m.provider, d.id AS revision_id, d.definition
FROM runner.runners r
JOIN model.platform_models m ON m.id = r.decision_platform_model_id
JOIN model.decision_revisions d
  ON d.id = m.decision_revision_id AND d.platform_model_id = m.id
WHERE r.id = $1 AND m.decision_enabled
