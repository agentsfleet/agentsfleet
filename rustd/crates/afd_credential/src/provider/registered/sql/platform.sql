WITH locked AS MATERIALIZED (
    SELECT p.provider, m.model_name AS model, m.id AS platform_model_id,
           m.context_cap_tokens AS context_window_tokens,
           s.id AS secret_id, s.meta_base_url, sha256(s.secret_ciphertext) AS secret_digest,
           r.resource_version, s.workspace_id::text AS workspace_id, s.key_name
      FROM model.platform_defaults p
      JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
      JOIN model.platform_models m ON m.id = p.platform_model_id AND m.provider = p.provider
     CROSS JOIN model.catalogue_version r
     WHERE p.active AND p.base_url IS NULL AND s.meta_has_key
       AND s.meta_provider = p.provider
       AND EXISTS (SELECT 1 FROM identity.tenants WHERE id = $1::uuid)
       AND $2::text IS NOT NULL
     ORDER BY p.provider LIMIT 1
       FOR SHARE OF p, s, m, r
), observed AS MATERIALIZED (
    SELECT clock_timestamp() AS read_at FROM locked LIMIT 1
)
SELECT l.provider, l.model, l.context_window_tokens,
       p.input_nanos_per_mtok AS input_nanos_per_million,
       p.output_nanos_per_mtok AS output_nanos_per_million,
       encode(sha256(convert_to(jsonb_build_array(
           l.provider, l.secret_id, l.provider, l.meta_base_url,
           encode(l.secret_digest, 'hex'), l.model, l.context_window_tokens,
           p.input_nanos_per_mtok, p.output_nanos_per_mtok,
           p.id, l.resource_version
       )::text, 'UTF8')), 'hex') AS resolution_version,
       l.workspace_id, l.key_name
  FROM locked l
 CROSS JOIN observed t
  JOIN model.platform_model_prices p ON p.platform_model_id = l.platform_model_id
   AND p.prompt_tokens_above = 0 AND p.effective_from <= t.read_at
   AND (p.effective_until IS NULL OR p.effective_until > t.read_at)
