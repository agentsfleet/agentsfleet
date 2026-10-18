DELETE FROM model.platform_model_prices AS p
USING model.platform_models AS m
WHERE p.platform_model_id = m.id
  AND NOT EXISTS (
      SELECT 1
      FROM jsonb_to_recordset($1::jsonb) AS wanted(
          provider text, model_name text, prompt_tokens_above integer, effective_from timestamptz
      )
      WHERE wanted.provider = m.provider AND wanted.model_name = m.model_name
        AND wanted.prompt_tokens_above = p.prompt_tokens_above
        AND wanted.effective_from = p.effective_from
  );
