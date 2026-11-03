SELECT m.id::text, m.provider, m.model_name, m.display_name, m.context_cap_tokens,
       m.context_source_url, m.context_verified_at, m.advanced, m.inference,
       CASE WHEN d.revision IS NULL THEN NULL
            ELSE jsonb_build_object('enabled', m.decision_enabled, 'profile', d.definition)
       END AS decision
FROM model.platform_models m
LEFT JOIN model.decision_revisions d
       ON d.platform_model_id = m.id AND d.id = m.decision_revision_id
ORDER BY m.provider, m.model_name
