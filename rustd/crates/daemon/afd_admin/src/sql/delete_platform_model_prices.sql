DELETE FROM model.platform_model_prices p
WHERE p.platform_model_id = $1::uuid
  AND (p.prompt_tokens_above, p.effective_from) NOT IN (
      SELECT * FROM unnest($2::integer[], $3::timestamptz[]))
