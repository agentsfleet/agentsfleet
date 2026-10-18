WITH configured AS (
    SELECT e.id::text AS reference,
           s.meta_provider AS provider,
           e.model_name AS model,
           s.meta_base_url AS base_url,
           s.meta_has_key AS has_key
      FROM model.tenant_models e
      JOIN LATERAL (
            SELECT id FROM identity.workspaces
             WHERE tenant_id = e.tenant_id
             ORDER BY created_at, id
             LIMIT 1
           ) w ON true
      JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
     WHERE e.tenant_id = $1::uuid
    UNION ALL
    (SELECT $2::text, p.provider, m.model_name, s.meta_base_url, s.meta_has_key
       FROM model.platform_defaults p
       JOIN model.platform_models m ON m.id = p.platform_model_id
       JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
      WHERE p.active AND s.meta_provider = p.provider
      ORDER BY p.provider
      LIMIT 1)
)
SELECT c.reference, c.provider, c.model, c.base_url, c.has_key,
       m.context_cap_tokens AS context,
       m.input_nanos_per_mtok AS input,
       m.output_nanos_per_mtok AS output,
       m.inference
  FROM configured c
  JOIN LATERAL (
        SELECT context_cap_tokens, input_nanos_per_mtok, output_nanos_per_mtok, inference
          FROM model.platform_models
         WHERE model_name = c.model AND provider = c.provider
       ) m ON true
 WHERE c.has_key
   AND m.context_cap_tokens > 0 AND m.input_nanos_per_mtok >= 0 AND m.output_nanos_per_mtok >= 0
   AND ($3::text IS NULL OR c.reference > $3)
 ORDER BY c.reference
 LIMIT $4
