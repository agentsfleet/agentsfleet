SELECT p.provider,
       m.model_name AS model,
       GREATEST(m.context_cap_tokens, 0) AS context_cap_tokens,
       encode(sha256(convert_to(jsonb_build_array(
           jsonb_build_array(p.provider, m.model_name, p.base_url, m.context_cap_tokens),
           s.id, s.kek_version, s.meta_provider, s.meta_base_url,
           encode(sha256(s.secret_ciphertext), 'hex'), r.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       s.workspace_id::text AS workspace_id,
       s.key_name,
       p.base_url AS endpoint
  FROM model.platform_defaults p
  JOIN model.platform_models m ON m.id = p.platform_model_id
  JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
 CROSS JOIN model.catalogue_version r
 WHERE p.active = true AND s.meta_provider = p.provider
   AND (s.meta_has_key OR p.provider = $3)
   AND EXISTS (SELECT 1 FROM identity.tenants WHERE id = $1::uuid)
   AND $2::text IS NOT NULL
 ORDER BY p.updated_at DESC, p.provider DESC
 LIMIT 1
   FOR SHARE OF p, s, r
