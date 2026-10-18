DELETE FROM model.platform_model_prices p
WHERE p.platform_model_id = $1::uuid
  AND NOT EXISTS (
      SELECT 1 FROM jsonb_to_recordset($2::jsonb) AS wanted(
          effective_from timestamptz, prompt_tokens_above integer)
      WHERE wanted.effective_from = p.effective_from
        AND wanted.prompt_tokens_above = p.prompt_tokens_above)
