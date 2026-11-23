WITH chosen AS MATERIALIZED (
    SELECT m.id
      FROM model.tenant_models m
     WHERE m.tenant_id = $1 AND m.model_name = $3 AND m.secret_ref = $4
       AND m.workspace_id = $5
       AND EXISTS (SELECT 1 FROM model.platform_models
                    WHERE provider = $2 AND model_name = $3)
), moved AS (
    UPDATE model.tenant_models t
       SET selected = (t.id = c.id), updated_at = now()
      FROM chosen c
     WHERE t.tenant_id = $1 AND (t.selected OR t.id = c.id)
    RETURNING t.id, t.selected
)
SELECT id AS tenant_model_id,
       GREATEST(COALESCE((SELECT MIN(context_cap_tokens)
                            FROM model.platform_models
                           WHERE model_name = $3
                             AND provider = $2), 0), 0) AS context_cap_tokens
  FROM moved
 WHERE selected
