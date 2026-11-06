SELECT m.inference,
       CASE WHEN d.revision IS NULL THEN NULL
            ELSE jsonb_build_object('enabled', m.decision_enabled, 'profile', d.definition)
       END AS decision
FROM model.platform_models m
LEFT JOIN model.decision_revisions d
       ON d.platform_model_id = m.id AND d.id = m.decision_revision_id
WHERE m.id = $1::uuid
FOR UPDATE OF m
