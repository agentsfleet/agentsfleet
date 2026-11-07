SELECT COALESCE(s.tenant_model_id::text, $2::text)
  FROM (SELECT) seed
  LEFT JOIN model.tenant_selections s ON s.tenant_id = $1::uuid
