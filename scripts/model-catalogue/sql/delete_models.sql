WITH wanted AS MATERIALIZED (
    SELECT provider, model_name FROM jsonb_to_recordset($1::jsonb) AS identity(provider text, model_name text)
), cleared AS (
    UPDATE model.platform_models AS retained
       SET inference = NULL, updated_at = now()
     WHERE retained.decision_revision_id IS NOT NULL
       AND retained.inference IS NOT NULL
       AND NOT EXISTS (
           SELECT 1 FROM wanted
           WHERE wanted.provider = retained.provider AND wanted.model_name = retained.model_name
       )
)
DELETE FROM model.platform_models AS m
WHERE m.decision_revision_id IS NULL
  AND NOT EXISTS (
    SELECT 1 FROM wanted
    WHERE wanted.provider = m.provider AND wanted.model_name = m.model_name
);
