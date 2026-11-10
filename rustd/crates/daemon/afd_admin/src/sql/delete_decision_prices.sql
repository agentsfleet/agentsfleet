DELETE FROM model.decision_prices p
WHERE p.platform_model_id = $1 AND p.decision_revision_id = $4
  AND (p.prompt_tokens_above, p.effective_from_at) NOT IN (
      SELECT * FROM unnest($2::integer[], $3::timestamptz[]))
