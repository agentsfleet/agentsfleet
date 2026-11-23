SELECT m.id AS tenant_model_id,
       m.model_name,
       m.secret_ref,
       m.workspace_id,
       s.meta_provider AS provider,
       GREATEST(COALESCE(c.cap, 0), 0) AS context_cap_tokens
  FROM model.tenant_models m
  LEFT JOIN vault.secrets s
         ON s.workspace_id = m.workspace_id AND s.key_name = m.secret_ref
  LEFT JOIN LATERAL (
        SELECT MIN(p.context_cap_tokens) AS cap
          FROM model.platform_models p
         WHERE p.model_name = m.model_name
           AND p.provider = s.meta_provider
       ) c ON true
 WHERE m.tenant_id = $1 AND m.selected
