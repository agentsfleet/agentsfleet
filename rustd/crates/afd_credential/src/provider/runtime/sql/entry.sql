SELECT s.meta_provider AS provider,
       e.model_name AS model,
       GREATEST(COALESCE(c.cap, 0), 0) AS context_cap_tokens,
       encode(sha256(convert_to(jsonb_build_array(
           jsonb_build_array(e.id, e.model_name, e.secret_ref, c.cap),
           s.id, s.kek_version, s.meta_provider, s.meta_base_url,
           encode(sha256(s.secret_ciphertext), 'hex'), r.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       s.workspace_id::text AS workspace_id,
       s.key_name,
       s.meta_base_url AS endpoint
  FROM model.tenant_models e
  JOIN LATERAL (
        SELECT id FROM identity.workspaces
         WHERE tenant_id = e.tenant_id
         ORDER BY created_at ASC, id ASC
         LIMIT 1
       ) w ON true
  JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
  LEFT JOIN LATERAL (
        SELECT MIN(context_cap_tokens) AS cap
          FROM model.platform_models
         WHERE model_name = e.model_name
           AND (provider = s.meta_provider OR s.meta_provider = $3)
       ) c ON true
 CROSS JOIN model.catalogue_version r
 WHERE e.tenant_id = $1::uuid AND e.id::text = $2
   AND (s.meta_has_key OR s.meta_provider = $3)
   AND (c.cap IS NOT NULL OR s.meta_provider = $3)
   FOR SHARE OF e, s, r
