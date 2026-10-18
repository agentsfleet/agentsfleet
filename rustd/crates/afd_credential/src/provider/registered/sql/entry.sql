SELECT s.meta_provider AS provider,
       e.model_name AS model,
       m.context_cap_tokens AS context_window_tokens,
       m.input_nanos_per_mtok AS input_nanos_per_million,
       m.output_nanos_per_mtok AS output_nanos_per_million,
       encode(sha256(convert_to(jsonb_build_array(
           e.id, s.id, s.meta_provider, s.meta_base_url,
           encode(sha256(s.secret_ciphertext), 'hex'),
           m.model_name, m.context_cap_tokens, m.input_nanos_per_mtok,
           m.output_nanos_per_mtok, r.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       s.workspace_id::text AS workspace_id,
       s.key_name
  FROM model.tenant_models e
  JOIN LATERAL (
        SELECT id FROM identity.workspaces
         WHERE tenant_id = e.tenant_id
         ORDER BY created_at ASC, id ASC
         LIMIT 1
       ) w ON true
  JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
  JOIN model.platform_models m ON m.provider = s.meta_provider AND m.model_name = e.model_name
 CROSS JOIN model.catalogue_version r
 WHERE e.tenant_id = $1::uuid AND e.id::text = $2
   AND s.meta_has_key = true AND s.meta_base_url IS NULL
   FOR SHARE OF e, s, m, r
