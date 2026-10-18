DELETE FROM model.platform_models AS m
WHERE NOT EXISTS (
    SELECT 1 FROM jsonb_to_recordset($1::jsonb) AS wanted(provider text, model_name text)
    WHERE wanted.provider = m.provider AND wanted.model_name = m.model_name
);
