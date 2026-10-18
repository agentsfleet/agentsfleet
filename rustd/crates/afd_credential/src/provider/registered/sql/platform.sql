SELECT p.provider,
       m.model_name AS model,
       m.context_cap_tokens AS context_window_tokens,
       m.input_nanos_per_mtok AS input_nanos_per_million,
       m.output_nanos_per_mtok AS output_nanos_per_million,
       encode(sha256(convert_to(jsonb_build_array(
           p.provider, s.id, s.meta_provider, s.meta_base_url,
           encode(sha256(s.secret_ciphertext), 'hex'),
           m.model_name, m.context_cap_tokens, m.input_nanos_per_mtok,
           m.output_nanos_per_mtok, r.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       s.workspace_id::text AS workspace_id,
       s.key_name
  FROM model.platform_defaults p
  JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
  JOIN model.platform_models m ON m.id = p.platform_model_id AND m.provider = p.provider
 CROSS JOIN model.catalogue_version r
 WHERE p.active = true AND p.base_url IS NULL AND s.meta_has_key = true
   AND s.meta_provider = p.provider
   AND EXISTS (SELECT 1 FROM identity.tenants WHERE id = $1::uuid)
   AND $2::text IS NOT NULL
 ORDER BY p.provider
 LIMIT 1
   FOR SHARE OF p, s, m, r
