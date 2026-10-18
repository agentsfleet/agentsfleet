WITH locked AS MATERIALIZED (
    SELECT s.meta_provider AS provider, e.model_name AS model,
           m.id AS platform_model_id, m.context_cap_tokens AS context_window_tokens,
           e.id AS entry_id, s.id AS secret_id, s.meta_base_url,
           sha256(s.secret_ciphertext) AS secret_digest,
           r.resource_version, s.workspace_id::text AS workspace_id, s.key_name,
           m.inference
      FROM model.tenant_models e
      JOIN LATERAL (
            SELECT id FROM identity.workspaces
             WHERE tenant_id = e.tenant_id
             ORDER BY created_at, id LIMIT 1
           ) w ON true
      JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
      JOIN model.platform_models m ON m.provider = s.meta_provider AND m.model_name = e.model_name
     CROSS JOIN model.catalogue_version r
     WHERE e.tenant_id = $1::uuid AND e.id::text = $2
       AND s.meta_has_key
       FOR SHARE OF e, s, m, r
), observed AS MATERIALIZED (
    SELECT clock_timestamp() AS read_at FROM locked LIMIT 1
)
SELECT l.provider, l.model, l.context_window_tokens,
       p.input_nanos_per_mtok AS input_nanos_per_million,
       p.output_nanos_per_mtok AS output_nanos_per_million,
       encode(sha256(convert_to(jsonb_build_array(
           l.entry_id, l.secret_id, l.provider, l.meta_base_url,
           encode(l.secret_digest, 'hex'), l.model, l.context_window_tokens,
           p.input_nanos_per_mtok, p.output_nanos_per_mtok,
           p.id, l.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       l.workspace_id, l.key_name,
       l.meta_base_url AS base_url,
       l.inference
  FROM locked l
 CROSS JOIN observed t
  JOIN model.platform_model_prices p ON p.platform_model_id = l.platform_model_id
   AND p.prompt_tokens_above = 0 AND p.effective_from <= t.read_at
   AND (p.effective_until IS NULL OR p.effective_until > t.read_at)
