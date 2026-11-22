SELECT COALESCE(m.id::text, $2::text)
  FROM (SELECT) seed
  LEFT JOIN model.tenant_models m ON m.tenant_id = $1 AND m.selected
