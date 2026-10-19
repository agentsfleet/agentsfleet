INSERT INTO model.tenant_selections (tenant_id, tenant_model_id, created_at, updated_at)
SELECT m.tenant_id, m.id, now(), now()
  FROM model.tenant_models m
 WHERE m.tenant_id = $1::uuid AND m.model_name = $3 AND m.secret_ref = $4
   AND ($5::boolean OR EXISTS (SELECT 1 FROM model.platform_models
                                WHERE provider = $2 AND model_name = $3))
ON CONFLICT (tenant_id) DO UPDATE SET
    tenant_model_id = EXCLUDED.tenant_model_id,
    updated_at = now()
RETURNING tenant_model_id::text AS tenant_model_id,
          GREATEST(COALESCE((SELECT MIN(context_cap_tokens)
                               FROM model.platform_models
                              WHERE model_name = $3
                                AND ($5::boolean OR provider = $2)), 0), 0) AS context_cap_tokens
